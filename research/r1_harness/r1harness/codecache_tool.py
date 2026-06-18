"""Adapter to the Rust ``codecache`` binary over a process boundary.

This is the A1/A4 retrieval interface: the harness (Python) shells out to the
built ``codecache`` binary (Rust). **No FFI, no PyO3, no new crate dependency**
— the zero-dependency single-binary identity (D12/D15) is preserved; the agent
consumes the binary exactly as a real user's agent would (D22).

``query(...)`` parses the binary's ``--format json`` output (§6.4.2:
``{query, total_results, total_tokens, chunks[]}`` with each chunk carrying
``symbol_name``, ``file_path``, ``bm25_score``, ``chunk_text``, …) and returns
the gold-comparable retrieved lists the Layer-1 scorer ranks.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path


def find_codecache_binary(repo_root: Path | None = None) -> Path:
    """Locate the built ``codecache`` binary.

    Order: ``$CODECACHE_BIN`` → ``target/release`` → ``target/debug`` → ``PATH``.
    Raises ``FileNotFoundError`` with actionable guidance if none is found.
    """
    env = os.environ.get("CODECACHE_BIN")
    if env and Path(env).exists():
        return Path(env)

    root = Path(repo_root) if repo_root else Path(__file__).resolve().parents[3]
    exe = "codecache.exe" if os.name == "nt" else "codecache"
    for profile in ("release", "debug"):
        cand = root / "target" / profile / exe
        if cand.exists():
            return cand

    on_path = shutil.which("codecache")
    if on_path:
        return Path(on_path)

    raise FileNotFoundError(
        "codecache binary not found. Build it with `cargo build --release`, or set $CODECACHE_BIN to its path."
    )


#: Indexed FTS5 columns the binary's ``bm25()`` weights map to, in ``schema::CREATE_SYMBOLS``
#: order: symbol_name, symbol_type, chunk_text, parent_symbol, imports, cross_references,
#: file_docstring. The ``--bm25-weights`` flag (R2.2a / D24) takes exactly this many
#: comma-separated numbers; the binary validates again and remains the source of truth.
N_BM25_COLUMNS = 7


def build_query_args(
    query: str,
    *,
    max_tokens: int = 4000,
    max_results: int = 20,
    bm25_weights: Sequence[float] | None = None,
) -> list[str]:
    """Build the ``codecache query`` argv (sans the binary path) for JSON output.

    With ``bm25_weights`` omitted, no ``--bm25-weights`` flag is emitted — the binary uses its
    built-in per-column defaults, the byte-identical default path (R2.2a). When supplied, the
    vector must have exactly :data:`N_BM25_COLUMNS` entries; since the sweep generates these
    programmatically, a wrong-length vector raises ``ValueError`` here rather than surfacing as
    an opaque subprocess failure. Each weight is normalised to ``float`` and comma-joined.
    """
    args = [
        "query",
        query,
        "--format",
        "json",
        "--max-tokens",
        str(max_tokens),
        "--max-results",
        str(max_results),
    ]
    if bm25_weights is not None:
        if len(bm25_weights) != N_BM25_COLUMNS:
            raise ValueError(
                f"bm25_weights must have exactly {N_BM25_COLUMNS} entries "
                f"(one per indexed FTS5 column), got {len(bm25_weights)}"
            )
        args += ["--bm25-weights", ",".join(str(float(w)) for w in bm25_weights)]
    return args


@dataclass
class QueryResult:
    """Parsed ``codecache query --format json`` output, gold-comparable."""

    query: str
    total_results: int
    total_tokens: int
    files: list[str]  # ordered, best-first, deduplicated by first occurrence
    blocks: list[tuple[str, str]]  # ordered (file_path, symbol_name), best-first
    raw: dict


class CodeCacheIndex:
    """A ``codecache`` index over a repo dir, driven via the CLI.

    Mirrors what arm A1 lets the agent do: ``init`` → ``index`` once, then
    ``query`` per turn. Paths are resolved against ``repo_dir`` (the binary
    operates on the working directory).
    """

    def __init__(self, repo_dir: Path, binary: Path | None = None, timeout: float = 120.0) -> None:
        self.repo_dir = Path(repo_dir)
        self.binary = Path(binary) if binary else find_codecache_binary()
        self.timeout = timeout

    def _run(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(self.binary), *args],
            cwd=str(self.repo_dir),
            capture_output=True,
            text=True,
            timeout=self.timeout,
            check=False,
        )

    def init(self) -> None:
        cp = self._run("init")
        if cp.returncode != 0:
            raise RuntimeError(f"codecache init failed ({cp.returncode}): {cp.stderr.strip()}")

    def index(self) -> None:
        cp = self._run("index")
        if cp.returncode != 0:
            raise RuntimeError(f"codecache index failed ({cp.returncode}): {cp.stderr.strip()}")

    def ingest(self, chunks_path: "Path | str") -> None:
        """Ingest pre-chunked records from a D25 JSON file, bypassing the native chunker.

        Shells ``codecache ingest <chunks_path>`` via the existing ``_run`` seam (D25/R2.3a).
        Raises ``RuntimeError`` if the subprocess exits non-zero, mirroring ``init``/``index``.

        Args:
            chunks_path: path to a JSON file containing a top-level array of D25 ingest records.
        """
        cp = self._run("ingest", str(chunks_path))
        if cp.returncode != 0:
            raise RuntimeError(f"codecache ingest failed ({cp.returncode}): {cp.stderr.strip()}")

    def query(
        self,
        query: str,
        *,
        max_tokens: int = 4000,
        max_results: int = 20,
        bm25_weights: Sequence[float] | None = None,
    ) -> QueryResult:
        cp = self._run(
            *build_query_args(
                query,
                max_tokens=max_tokens,
                max_results=max_results,
                bm25_weights=bm25_weights,
            )
        )
        if cp.returncode != 0:
            raise RuntimeError(f"codecache query failed ({cp.returncode}): {cp.stderr.strip()}")
        return parse_query_json(cp.stdout, query, repo_dir=self.repo_dir)


def normalize_path(file_path: str, repo_dir: Path | None) -> str:
    """Relativise a retrieved ``file_path`` to ``repo_dir`` as a posix string.

    The binary may emit absolute paths (it indexes the working directory); gold
    contexts are repo-relative posix paths (``src/auth/authenticate.py``), so we
    relativise + normalise separators to make the gold comparison apples-to-apples.
    Falls back to a posix-normalised original if it is not under ``repo_dir``.
    """
    p = Path(file_path)
    if repo_dir is not None:
        try:
            return p.resolve().relative_to(Path(repo_dir).resolve()).as_posix()
        except (ValueError, OSError):
            pass
    return p.as_posix()


def parse_query_json(stdout: str, query: str, repo_dir: Path | None = None) -> QueryResult:
    """Parse §6.4.2 JSON into the gold-comparable :class:`QueryResult`.

    File paths are relativised to ``repo_dir`` (see :func:`normalize_path`).
    File-level list is deduplicated by first occurrence (matching the Rust
    ``score_corpus`` fold); block-level list keeps full best-first order.
    """
    obj = json.loads(stdout)
    chunks = obj.get("chunks", [])
    files: list[str] = []
    blocks: list[tuple[str, str]] = []
    for c in chunks:
        fp = normalize_path(c["file_path"], repo_dir)
        blocks.append((fp, c["symbol_name"]))
        if fp not in files:
            files.append(fp)
    return QueryResult(
        query=obj.get("query", query),
        total_results=obj.get("total_results", len(chunks)),
        total_tokens=obj.get("total_tokens", 0),
        files=files,
        blocks=blocks,
        raw=obj,
    )

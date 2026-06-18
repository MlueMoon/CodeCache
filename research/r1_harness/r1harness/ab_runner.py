"""A/B runner for R2.3b/R2.6: compare chunker arms over one corpus.

Both arms run over the SAME materialised on-disk files, scored by the EXISTING scorer
(scorer.score_query / macro_average) against the SAME gold from the micro-suite.

R2.3b arms (run_ab):
- **native**: init → index → query   (CodeCache's built-in Tree-sitter chunker)
- **stub**:   init → ingest <stub.json> → query  (stub_chunk output via the D25 seam)

R2.6 arms (run_ab_astchunk):
- **native**:   init → index → query   (CodeCache's built-in Tree-sitter chunker)
- **astchunk**: init → ingest <astchunk_chunks.json> → query  (astchunk cAST chunker)

Constraints for R2.6:
- Only Python and TypeScript corpora are supported; Go (``data_processing``) must be skipped
  at the caller — astchunk 0.1.0 has no Go grammar.
- Report file-level metrics as the headline; block-level is ~0 because synthesised symbol
  names (``"{file_path}::L{start_line}-L{end_line}"``) never match real gold block keys.

The runner does NOT assert a winner — it is outcome-agnostic (project_overview §7).
Whether any arm beats native is an R3 determination; R2 proves the plumbing and measures.

Row shape (one dict per arm)::

    {
        "arm":       str,
        "macro_all": {k: MetricAtK, ...},   # macro_average() output keyed by int k
        "n_queries": int,
    }
"""

from __future__ import annotations

from pathlib import Path
from typing import Sequence

from .astchunk_chunker import astchunk_chunk
from .chunkers import dump_chunks, stub_chunk
from .codecache_tool import CodeCacheIndex
from .corpus import Corpus, materialize
from .scorer import MetricAtK, dedup_first, macro_average, score_query
from .sweep import SweepQuery, load_suite


def _load_queries_for_corpus(corpus_id: str) -> list[SweepQuery]:
    """Load the micro-suite queries for a specific corpus id."""
    all_queries = load_suite()
    return [q for q in all_queries if q.corpus_id == corpus_id]


def _score_arm(
    idx: CodeCacheIndex,
    queries: Sequence[SweepQuery],
) -> tuple[dict[int, MetricAtK], int]:
    """Score one arm: run each query, score against gold, macro-average.

    Returns:
        (macro_all, n_queries) where macro_all is the macro_average() dict.
    """
    per_query: list[list[MetricAtK]] = []
    for sq in queries:
        result = idx.query(sq.query)
        metrics = score_query(
            dedup_first(result.files),
            list(result.blocks),
            set(sq.gold_files),
            set(sq.gold_blocks),
        )
        per_query.append(metrics)
    return macro_average(per_query), len(per_query)


def run_ab(
    corpus: Corpus,
    binary: Path,
    work_dir: Path,
    queries: list[SweepQuery] | None = None,
) -> list[dict]:
    """Run the native and stub arms over ``corpus``, score both against the same gold.

    Args:
        corpus:    the Corpus whose chunks are used for both arms.
        binary:    path to the runnable ``codecache`` binary.
        work_dir:  scratch directory; both arms' repo trees + DB are created here.
        queries:   gold-labelled queries to score; if None, loads from the micro-suite
                   for ``corpus.id``; if ``[]``, scores against empty (n_queries == 0).

    Returns:
        A list of exactly two row dicts (one per arm: ``"native"`` and ``"stub"``), each
        carrying ``arm``, ``macro_all``, and ``n_queries`` keys.  No winner is asserted.
    """
    work_dir = Path(work_dir)

    # Resolve query list.
    if queries is None:
        arm_queries: Sequence[SweepQuery] = _load_queries_for_corpus(corpus.id)
    else:
        arm_queries = queries

    rows: list[dict] = []

    # --- Native arm: init → index → query ---
    native_dir = work_dir / "native" / "repo"
    native_dir.mkdir(parents=True, exist_ok=True)
    materialize(corpus, native_dir)
    native_idx = CodeCacheIndex(repo_dir=native_dir, binary=binary)
    native_idx.init()
    native_idx.index()
    native_macro, native_n = _score_arm(native_idx, arm_queries)
    rows.append(
        {
            "arm": "native",
            "macro_all": native_macro,
            "n_queries": native_n,
        }
    )

    # --- Stub arm: init → ingest stub.json → query ---
    stub_dir = work_dir / "stub" / "repo"
    stub_dir.mkdir(parents=True, exist_ok=True)
    materialize(corpus, stub_dir)
    stub_records = stub_chunk(corpus)
    chunks_json = work_dir / "stub_chunks.json"
    dump_chunks(stub_records, chunks_json)
    stub_idx = CodeCacheIndex(repo_dir=stub_dir, binary=binary)
    stub_idx.init()
    stub_idx.ingest(chunks_json)
    stub_macro, stub_n = _score_arm(stub_idx, arm_queries)
    rows.append(
        {
            "arm": "stub",
            "macro_all": stub_macro,
            "n_queries": stub_n,
        }
    )

    return rows


def _astchunk_chunk_corpus(corpus: Corpus, language: str, max_chunk_size: int = 300) -> list[dict]:
    """Chunk all files in a corpus using astchunk and return D25 records.

    Reads each file's content from the corpus (by concatenating fixture chunks in order,
    matching how :func:`~corpus.materialize` builds the on-disk file), then calls
    :func:`~astchunk_chunker.astchunk_chunk` per file.

    Args:
        corpus:         Corpus to chunk.
        language:       ``"python"`` or ``"typescript"`` — passed to astchunk.
                        Raises ``ValueError`` for ``"go"`` (no astchunk grammar).
        max_chunk_size: Non-whitespace char budget (astchunk hyperparameter; default 300).

    Returns:
        Flat list of D25 record dicts for all files in the corpus.
    """
    by_file: dict[str, list[str]] = {}
    for chunk in corpus.chunks:
        by_file.setdefault(chunk["file_path"], []).append(chunk["chunk_text"])

    records: list[dict] = []
    for fp in corpus.files:  # preserve first-seen order (deterministic)
        content = "".join(by_file[fp])
        records.extend(astchunk_chunk(content, fp, language, max_chunk_size=max_chunk_size))
    return records


def run_ab_astchunk(
    corpus: Corpus,
    language: str,
    binary: Path,
    work_dir: Path,
    queries: list[SweepQuery] | None = None,
    max_chunk_size: int = 300,
) -> list[dict]:
    """Run the native and astchunk arms over ``corpus``, score both against the same gold.

    The native arm uses ``init → index → query`` (CodeCache's built-in Tree-sitter chunker).
    The astchunk arm uses ``init → ingest <astchunk_chunks.json> → query`` (the cAST chunker
    from the astchunk PyPI package via the D25 ingest seam).

    Both arms are scored by the identical Layer-1/NDCG scorer against the identical gold.
    No winner is asserted — outcome-agnostic (project_overview §7).

    Args:
        corpus:         Corpus to run (Python or TypeScript only — Go not supported).
        language:       ``"python"`` or ``"typescript"``; raises ``ValueError`` for ``"go"``.
        binary:         Path to the runnable ``codecache`` binary.
        work_dir:       Scratch directory; arm sub-dirs + DB created here.
        queries:        Gold-labelled queries; if None, loads from the micro-suite for
                        ``corpus.id``; if ``[]``, scores against empty (n_queries == 0).
        max_chunk_size: astchunk non-whitespace char budget (default 300; hyperparameter).

    Returns:
        A list of exactly two row dicts (``"native"`` and ``"astchunk"``), each carrying
        ``arm``, ``macro_all`` ({int: MetricAtK}), and ``n_queries``.

    Raises:
        ValueError: if ``language`` is not ``"python"`` or ``"typescript"``.
    """
    work_dir = Path(work_dir)

    # Resolve query list.
    if queries is None:
        arm_queries: Sequence[SweepQuery] = _load_queries_for_corpus(corpus.id)
    else:
        arm_queries = queries

    rows: list[dict] = []

    # --- Native arm: init → index → query ---
    native_dir = work_dir / "native_ast" / "repo"
    native_dir.mkdir(parents=True, exist_ok=True)
    materialize(corpus, native_dir)
    native_idx = CodeCacheIndex(repo_dir=native_dir, binary=binary)
    native_idx.init()
    native_idx.index()
    native_macro, native_n = _score_arm(native_idx, arm_queries)
    rows.append(
        {
            "arm": "native",
            "macro_all": native_macro,
            "n_queries": native_n,
        }
    )

    # --- astchunk arm: init → ingest astchunk_chunks.json → query ---
    # Chunk the SAME materialised files (content identical to what native arm indexes).
    astchunk_dir = work_dir / "astchunk" / "repo"
    astchunk_dir.mkdir(parents=True, exist_ok=True)
    materialize(corpus, astchunk_dir)
    astchunk_records = _astchunk_chunk_corpus(corpus, language, max_chunk_size=max_chunk_size)
    chunks_json = work_dir / "astchunk_chunks.json"
    dump_chunks(astchunk_records, chunks_json)
    astchunk_idx = CodeCacheIndex(repo_dir=astchunk_dir, binary=binary)
    astchunk_idx.init()
    astchunk_idx.ingest(chunks_json)
    astchunk_macro, astchunk_n = _score_arm(astchunk_idx, arm_queries)
    rows.append(
        {
            "arm": "astchunk",
            "macro_all": astchunk_macro,
            "n_queries": astchunk_n,
        }
    )

    return rows

"""R2.5a — ContextBench-Lite fetch entrypoint.

Downloads the ``contextbench_verified`` (Lite, 500-task) split from the HF dataset
``Contextbench/ContextBench`` ONCE at an explicit ``--revision``, writes the cached slice as a
JSON list under the cache dir plus a sidecar provenance file, and exits.  Subsequent runs skip
re-download if the cache is already present.

This is the ONLY network surface in R2.5a.  The test suite (pytest) NEVER calls this
script — tests run against fixture data.  The pure-logic mapper (``r1harness/contextbench.py``)
has no I/O and is tested independently.

Reproducibility (review follow-up 2026-06-17):
    The fetch is pinned to ``--revision`` (default ``"main"``; pass an explicit commit SHA for a
    fully-reproducible corpus).  A sidecar ``contextbench_verified_slice.meta.json`` records the
    dataset/config/split, the *requested* revision, and the *resolved* commit SHA (best-effort via
    huggingface_hub; ``None`` when offline).  The records-list cache itself stays a bare JSON array,
    so downstream ``json.loads → list`` loaders are unchanged.

Usage:
    python3 fetch_contextbench.py [--cache-dir PATH] [--n-records N] [--revision REV] [--force]

Options:
    --cache-dir PATH   Directory to write the cached slice (default: ./cache/contextbench)
    --n-records N      Number of records to cache (default: 20; full Lite = 500)
    --revision REV     HF dataset revision — branch/tag/commit SHA (default: "main")
    --force            Re-download even if cache exists

Environment variables:
    CONTEXTBENCH_CACHE  Override default cache dir (same as --cache-dir)

Exit codes:
    0  Success (downloaded or cache already present)
    1  Error (network failure, missing deps, etc.)

Missing-cache behaviour for downstream scripts:
    If the cache does not exist, downstream scripts should call this entrypoint first.
    See the stderr message in load_cached_contextbench() for instructions.

Dataset:  HF ``Contextbench/ContextBench``, config ``contextbench_verified``.
License:  Apache-2.0.  arXiv:2602.05892.  No auth token required.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

DEFAULT_CACHE_DIR = Path(__file__).resolve().parent / "cache" / "contextbench"
DEFAULT_N_RECORDS = 20
CACHE_FILE_NAME = "contextbench_verified_slice.json"
#: Sidecar provenance file written ALONGSIDE the records list (the list cache stays a bare JSON
#: array so downstream `json.loads → list` loaders are unaffected). Records the pinned revision.
PROVENANCE_FILE_NAME = "contextbench_verified_slice.meta.json"
#: HF dataset coordinates — pinned so a cached slice is reproducible.
DATASET_REPO = "Contextbench/ContextBench"
DATASET_CONFIG = "contextbench_verified"
DATASET_SPLIT = "train"
#: Default dataset revision. "main" pins the branch; pass an explicit commit SHA via `--revision`
#: for a fully-reproducible fetch (the resolved commit is recorded in the sidecar regardless).
DEFAULT_REVISION = "main"


def _cache_path(cache_dir: Path) -> Path:
    return cache_dir / CACHE_FILE_NAME


def _provenance_path(cache_dir: Path) -> Path:
    return cache_dir / PROVENANCE_FILE_NAME


def build_provenance(
    *,
    dataset: str,
    config: str,
    split: str,
    revision: str,
    resolved_commit: str | None,
    n_cached: int,
    n_total: int,
) -> dict:
    """Assemble the provenance record for a cached ContextBench-Lite slice (pure, no I/O).

    Captures exactly what is needed to reproduce a fetch: the dataset coordinates, the
    *requested* revision (branch or SHA), the *resolved* commit SHA when the hub could supply it
    (else ``None``), and the cached/total record counts. Deterministic and JSON-serialisable; it
    is written as a sidecar ``<slice>.meta.json`` so the records-list cache file stays a bare
    array (downstream loaders read it with ``json.loads`` and expect a ``list``).
    """
    return {
        "dataset": dataset,
        "config": config,
        "split": split,
        "revision_requested": revision,
        "revision_resolved": resolved_commit,
        "n_cached": n_cached,
        "n_total": n_total,
    }


def _resolve_commit(dataset: str, revision: str) -> str | None:
    """Best-effort resolve *revision* (a branch/tag/SHA) to a concrete commit SHA via the HF Hub.

    Returns the SHA string, or ``None`` if huggingface_hub is unavailable or the lookup fails
    (offline, older hub, gated repo). Never raises — provenance degrades to the requested
    revision rather than failing the fetch.
    """
    try:
        from huggingface_hub import HfApi  # type: ignore[import]

        return HfApi().dataset_info(dataset, revision=revision).sha
    except Exception:
        return None


def load_cached_contextbench(cache_dir: Path | None = None) -> list[dict]:
    """Load the cached ContextBench-Lite slice as a list of dicts.

    Called by downstream scripts (not by the test suite).  If the cache is missing,
    prints a clear instruction to stderr and raises SystemExit(1).
    """
    resolved = Path(cache_dir) if cache_dir else DEFAULT_CACHE_DIR
    cp = _cache_path(resolved)
    if not cp.exists():
        print(
            "ERROR: ContextBench-Lite cache not found.\n"
            f"  Expected: {cp}\n"
            "  Run the fetch entrypoint first:\n"
            "    python3 fetch_contextbench.py\n"
            "  Then retry.",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return json.loads(cp.read_text(encoding="utf-8"))


def fetch_and_cache(
    cache_dir: Path,
    n_records: int,
    force: bool = False,
    revision: str = DEFAULT_REVISION,
) -> int:
    """Download the ContextBench-Lite slice at ``revision`` and write it to ``cache_dir``.

    Writes two files: the records list (``contextbench_verified_slice.json`` — a bare JSON array,
    unchanged shape) and a sidecar provenance record (``…_slice.meta.json``) pinning the dataset
    coordinates + the requested/resolved revision. Returns 0 on success, 1 on failure.
    """
    cp = _cache_path(cache_dir)

    if cp.exists() and not force:
        print(f"Cache already present: {cp}", file=sys.stderr)
        return 0

    # Import the HF stack only here — keeps the mapper and test suite deps-free.
    try:
        from datasets import load_dataset  # type: ignore[import]
    except ImportError:
        print(
            "ERROR: 'datasets' package not installed.\n"
            "  Install it with:  pip install datasets==5.0.0 huggingface_hub==1.19.0\n"
            "  Then retry:       python3 fetch_contextbench.py",
            file=sys.stderr,
        )
        return 1

    print(
        f"Downloading ContextBench-Lite ({DATASET_CONFIG}@{revision}, up to {n_records} records) from HF (no auth token)...",
        file=sys.stderr,
    )
    try:
        ds = load_dataset(
            DATASET_REPO,
            name=DATASET_CONFIG,
            split=DATASET_SPLIT,
            revision=revision,
            trust_remote_code=False,
        )
    except Exception as exc:
        print(f"ERROR: download failed: {exc}", file=sys.stderr)
        return 1

    # Take a deterministic head slice.
    total = len(ds)
    take = min(n_records, total)
    slice_ds = ds.select(range(take))

    # Materialise to a list of plain dicts (JSON-serialisable).
    records: list[dict] = []
    for row in slice_ds:
        records.append(dict(row))

    cache_dir.mkdir(parents=True, exist_ok=True)
    cp.write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding="utf-8")

    # Sidecar provenance — pins what was fetched (dataset/config/split + requested & resolved
    # revision) WITHOUT touching the records-list cache shape. Best-effort commit resolution so
    # the run records the exact SHA when the hub can supply it (offline → None, still reproducible
    # by branch). Written next to the slice so a reader can audit/repin the corpus.
    resolved_commit = _resolve_commit(DATASET_REPO, revision)
    provenance = build_provenance(
        dataset=DATASET_REPO,
        config=DATASET_CONFIG,
        split=DATASET_SPLIT,
        revision=revision,
        resolved_commit=resolved_commit,
        n_cached=take,
        n_total=total,
    )
    _provenance_path(cache_dir).write_text(
        json.dumps(provenance, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(
        f"Cached {take}/{total} records to: {cp}\n"
        f"  revision: {revision}"
        + (f" (resolved {resolved_commit})" if resolved_commit else " (commit unresolved)")
        + f"\n  provenance: {_provenance_path(cache_dir)}",
        file=sys.stderr,
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Fetch and cache a ContextBench-Lite slice from HF.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=Path(os.environ.get("CONTEXTBENCH_CACHE", str(DEFAULT_CACHE_DIR))),
        help=f"Cache directory (default: {DEFAULT_CACHE_DIR}; env: CONTEXTBENCH_CACHE)",
    )
    parser.add_argument(
        "--n-records",
        type=int,
        default=DEFAULT_N_RECORDS,
        help=f"Number of records to cache (default: {DEFAULT_N_RECORDS}; full Lite = 500)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Re-download even if cache already exists",
    )
    parser.add_argument(
        "--revision",
        type=str,
        default=DEFAULT_REVISION,
        help=(
            f"HF dataset revision: branch, tag, or commit SHA (default: {DEFAULT_REVISION!r}). "
            "Pass an explicit commit SHA for a fully-reproducible fetch; the resolved SHA is "
            "recorded in the sidecar provenance file regardless."
        ),
    )
    args = parser.parse_args(argv)

    cache_dir: Path = args.cache_dir
    cp = _cache_path(cache_dir)

    if args.force:
        # Explicit download requested — fetch and cache (imports datasets only here).
        return fetch_and_cache(
            cache_dir=cache_dir,
            n_records=args.n_records,
            force=True,
            revision=args.revision,
        )

    # Default read path (no --force): check the cache and instruct-and-exit if missing.
    # NEVER auto-download on the default path — mirror the run_report.py precedent.
    if not cp.exists():
        print(
            "ERROR: ContextBench-Lite cache not found.\n"
            f"  Expected: {cp}\n"
            "  Run the fetch entrypoint first:\n"
            "    python3 fetch_contextbench.py --force\n"
            "  Then retry.",
            file=sys.stderr,
        )
        return 1

    print(f"Cache already present: {cp}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

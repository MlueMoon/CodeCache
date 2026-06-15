"""A/B runner for R2.3b: compare the native and stub chunker arms over one corpus.

Both arms run over the SAME materialised on-disk files, scored by the EXISTING scorer
(scorer.score_query / macro_average) against the SAME gold from the micro-suite.

Arms:
- **native**: init → index → query   (CodeCache's built-in Tree-sitter chunker)
- **stub**:   init → ingest <stub.json> → query  (stub_chunk output via the D25 seam)

The runner does NOT assert a winner — it is outcome-agnostic (project_overview §7).
Whether stub beats native is an R3 determination; R2.3b proves the plumbing.

Row shape (one dict per arm)::

    {
        "arm":       "native" | "stub",
        "macro_all": {k: MetricAtK, ...},   # macro_average() output keyed by int k
        "n_queries": int,
    }
"""

from __future__ import annotations

from pathlib import Path
from typing import Sequence

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

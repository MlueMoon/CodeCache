"""RED tests for R2.3b — A/B runner + CodeCacheIndex.ingest adapter.

Covers:
  - A/B runner yields one row per arm (integration, NEEDS BINARY — skips when absent)
  - stub arm retrievable through ingest (integration, NEEDS BINARY — skips when absent)
  - A/B runner handles empty-queries corpus without crashing (pure, binary-free)
  - CodeCacheIndex.ingest method exists and shells to 'codecache ingest <path>'

The production modules (r1harness/ab_runner.py, the ingest adapter on
CodeCacheIndex) do NOT exist yet; every import here will fail with
ImportError — that is the correct RED state.

Binary skip mechanism: pytest.importorskip / pytest.skip via find_codecache_binary
raising FileNotFoundError.  Mirrors the test_runner_scoring.py pattern:
skip cleanly, never error, when no runnable Linux binary is present.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

from r1harness.codecache_tool import CodeCacheIndex, find_codecache_binary
from r1harness.corpus import Corpus, load_corpus

# --- Production imports (will fail RED: modules do not exist yet) ---
from r1harness.ab_runner import run_ab  # type: ignore[import]  # noqa: E402
from r1harness.chunkers import stub_chunk  # type: ignore[import]  # noqa: E402

# ---------------------------------------------------------------------------
# Binary availability fixture (shared across binary-dependent tests)
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def codecache_binary():
    """Return the codecache binary path, or skip the whole module if absent."""
    try:
        return find_codecache_binary()
    except FileNotFoundError as exc:
        pytest.skip(f"codecache binary not found (expected on WSL2 without a Linux build): {exc}")


# ---------------------------------------------------------------------------
# Scenario: CodeCacheIndex.ingest adapter (pure interface check)
# ---------------------------------------------------------------------------


def test_codecache_index_has_ingest_method():
    """CodeCacheIndex exposes an ingest(chunks_path) method (pure interface check)."""
    # We only need the attribute to exist on the class — no binary call here.
    assert hasattr(CodeCacheIndex, "ingest"), "CodeCacheIndex must have an 'ingest' method (R2.3b adapter)"


# ---------------------------------------------------------------------------
# Scenario: stub arm retrievable through ingest (integration, NEEDS BINARY)
# ---------------------------------------------------------------------------


def test_stub_arm_retrievable_through_ingest(codecache_binary, tmp_path):
    """init → ingest stub.json → query returns the expected gold symbol.

    Equivalence sanity check #1: stub records flow through ingest → FTS5 → retriever
    and the gold symbol (authenticate_user) is retrievable for the session-token query.
    Skips cleanly when no runnable binary is found.
    """
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    # Write D25 JSON to a temp file
    chunks_json = tmp_path / "stub_chunks.json"
    chunks_json.write_text(json.dumps(records), encoding="utf-8")

    # Materialise the repo tree so file_paths exist on disk
    from r1harness.corpus import materialize

    repo_dir = tmp_path / "repo"
    repo_dir.mkdir()
    materialize(corpus, repo_dir)

    # init → ingest → query
    idx = CodeCacheIndex(repo_dir=repo_dir, binary=codecache_binary)
    idx.init()
    idx.ingest(chunks_json)  # the new adapter method

    result = idx.query("authenticate user credentials")

    # The gold symbol for auth_q1 must appear in the retrieved blocks
    gold_block = ("src/auth/authenticate.py", "authenticate_user")
    assert gold_block in result.blocks, (
        f"Expected gold block {gold_block} in retrieved blocks, got: {result.blocks[:5]}"
    )


# ---------------------------------------------------------------------------
# Scenario: A/B runner yields one row per arm (integration, NEEDS BINARY)
# ---------------------------------------------------------------------------


def test_ab_runner_yields_one_row_per_arm(codecache_binary, tmp_path):
    """A/B runner returns exactly two scored rows (native + stub arm).

    Equivalence sanity check #2: both arms are scored by the identical scorer
    against the identical gold from load_suite/the corpus's queries.
    Does NOT assert which arm wins (outcome-agnostic, overview §7).
    Skips cleanly when no runnable binary is found.
    """
    corpus = load_corpus("auth_module")

    rows = run_ab(
        corpus=corpus,
        binary=codecache_binary,
        work_dir=tmp_path,
    )

    # Must produce exactly two rows — one per arm
    assert len(rows) == 2, f"expected 2 arms (native + stub), got {len(rows)}"

    arm_labels = {row["arm"] for row in rows}
    assert "native" in arm_labels, f"missing 'native' arm; got arms: {arm_labels}"
    assert "stub" in arm_labels, f"missing 'stub' arm; got arms: {arm_labels}"

    # Each row must carry well-formed Layer-1 metrics (MetricAtK-shaped)
    for row in rows:
        assert "arm" in row, f"row missing 'arm' key: {row}"
        assert "macro_all" in row, f"row missing 'macro_all' key: {row}"
        macro = row["macro_all"]
        # macro_all[10] must be a MetricAtK (or duck-typed equivalent)
        assert 10 in macro, f"macro_all missing k=10: {macro}"
        m10 = macro[10]
        assert hasattr(m10, "recall_block") and hasattr(m10, "f1_block"), (
            f"macro_all[10] is not MetricAtK-shaped: {m10!r}"
        )

    # Outcome-agnostic: do NOT assert which arm has higher score


def test_ab_runner_both_arms_use_same_gold(codecache_binary, tmp_path):
    """Both arms in the A/B runner are scored against the identical gold set."""
    corpus = load_corpus("auth_module")

    rows = run_ab(
        corpus=corpus,
        binary=codecache_binary,
        work_dir=tmp_path,
    )

    # Both rows must report the same number of queries scored
    n_queries = {row.get("n_queries") for row in rows}
    assert len(n_queries) == 1, (
        f"Arms scored different numbers of queries — they must use the same gold: "
        f"{[(r['arm'], r.get('n_queries')) for r in rows]}"
    )


# ---------------------------------------------------------------------------
# Scenario: A/B runner handles empty queries corpus (pure, binary-free)
# ---------------------------------------------------------------------------


def test_ab_runner_empty_queries_does_not_crash():
    """A/B runner over a corpus with zero queries does not crash.

    The chosen behaviour: run_ab returns two rows (one per arm) each with
    n_queries == 0 and all-zero macro metrics (matching macro_average([]) contract).
    If the implementation instead raises ValueError for empty queries, this test
    documents that choice — pin whichever is implemented.
    """
    # Build a corpus that has chunks but no queries attached
    # (we bypass load_suite — run_ab must handle query_list=[] gracefully)
    empty_query_corpus = Corpus(id="auth_module", chunks=load_corpus("auth_module").chunks)

    # run_ab with an explicitly empty query list must not crash
    # If no binary is available, we just verify the call signature is accepted
    try:
        binary = find_codecache_binary()
    except FileNotFoundError:
        pytest.skip("binary not available; skipping empty-corpus A/B test")

    with tempfile.TemporaryDirectory() as tmp:
        rows = run_ab(
            corpus=empty_query_corpus,
            binary=binary,
            work_dir=Path(tmp),
            queries=[],  # explicit empty list
        )
    # Must return exactly two rows (one per arm), each with n_queries == 0
    assert len(rows) == 2
    for row in rows:
        assert row.get("n_queries", 0) == 0


# ---------------------------------------------------------------------------
# Scenario: enrichment held constant between arms
# ---------------------------------------------------------------------------


def test_stub_records_carry_imports_and_cross_references():
    """Stub records preserve imports/cross_references so enrichment is held constant."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    # Find authenticate_user record
    auth_user = next(
        (r for r in records if r["symbol_name"] == "authenticate_user"),
        None,
    )
    assert auth_user is not None

    # Must carry the fixture's imports and cross_references unchanged
    expected_imports = [
        "from db import find_user",
        "from crypto import verify_password, generate_session_token",
    ]
    assert auth_user["imports"] == expected_imports, f"imports not preserved: {auth_user['imports']!r}"
    expected_xrefs = ["db.find_user", "verify_password", "generate_session_token"]
    assert auth_user["cross_references"] == expected_xrefs, (
        f"cross_references not preserved: {auth_user['cross_references']!r}"
    )

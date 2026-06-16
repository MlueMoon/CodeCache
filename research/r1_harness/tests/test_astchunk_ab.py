"""RED tests for R2.6 — astchunk A/B arm (run_ab_astchunk / ab_runner extension).

Covers:
  - run_ab_astchunk returns exactly 2 rows: "native" and "astchunk" arms
  - Go corpus is skipped (ValueError / explicit skip) — constraint from R2.6
  - Both arms carry the required row shape (arm, macro_all, n_queries)
  - astchunk arm uses 'init → ingest astchunk_chunks.json → query' path
  - Outcome-agnostic: no winner assertion
  - Pure-logic: astchunk_chunk_corpus is callable and returns a list of D25 records
  - Integration (NEEDS BINARY): astchunk arm retrieval is scoreable
  - Go-corpus language guard: astchunk_chunk_corpus raises ValueError for 'go' language

The production function (r1harness.ab_runner.run_ab_astchunk) does NOT exist yet;
every import here will fail with ImportError — that is the correct RED state.
"""

from __future__ import annotations

import json

import pytest

from r1harness.codecache_tool import find_codecache_binary
from r1harness.corpus import Corpus, load_corpus

# --- Production imports (will fail RED) ---
from r1harness.ab_runner import run_ab_astchunk  # type: ignore[import]
from r1harness.astchunk_chunker import astchunk_chunk  # already implemented


# ---------------------------------------------------------------------------
# Binary availability fixture
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def codecache_binary():
    """Return the codecache binary, or skip the module if absent."""
    try:
        return find_codecache_binary()
    except FileNotFoundError as exc:
        pytest.skip(f"codecache binary not found: {exc}")


# ---------------------------------------------------------------------------
# Helper: astchunk_chunk_corpus (pure, binary-free)
# ---------------------------------------------------------------------------


def _astchunk_chunk_corpus(corpus: Corpus, language: str, max_chunk_size: int = 300) -> list[dict]:
    """Chunk all files in a corpus with astchunk (per-file).

    Pure function, no binary calls.  For the Go language, astchunk_chunk raises
    ValueError — that is the documented constraint.
    """
    from r1harness.corpus import materialize as _  # noqa: F401 (ensure importable)

    # Build per-file content
    by_file: dict[str, list[str]] = {}
    for chunk in corpus.chunks:
        by_file.setdefault(chunk["file_path"], []).append(chunk["chunk_text"])

    records: list[dict] = []
    for fp, texts in by_file.items():
        content = "".join(texts)
        records.extend(astchunk_chunk(content, fp, language, max_chunk_size=max_chunk_size))
    return records


# ---------------------------------------------------------------------------
# Scenario 1 — pure-logic interface checks (binary-free)
# ---------------------------------------------------------------------------


def test_run_ab_astchunk_function_exists():
    """run_ab_astchunk is importable and callable."""
    assert callable(run_ab_astchunk), "run_ab_astchunk must be a callable"


def test_astchunk_chunk_corpus_python_nonempty():
    """_astchunk_chunk_corpus produces at least 1 record for the auth_module corpus."""
    corpus = load_corpus("auth_module")
    records = _astchunk_chunk_corpus(corpus, "python", max_chunk_size=300)
    assert isinstance(records, list)
    assert len(records) >= 1, "expected at least 1 record, got 0"


def test_astchunk_chunk_corpus_typescript_nonempty():
    """_astchunk_chunk_corpus produces at least 1 record for the config_module corpus."""
    corpus = load_corpus("config_module")
    records = _astchunk_chunk_corpus(corpus, "typescript", max_chunk_size=300)
    assert isinstance(records, list)
    assert len(records) >= 1, "expected at least 1 record, got 0"


def test_astchunk_go_raises_value_error():
    """astchunk_chunk raises ValueError for language='go' (not supported by astchunk 0.1.0)."""
    corpus = load_corpus("data_processing")
    go_chunk = corpus.chunks[0]
    content = go_chunk["chunk_text"]
    with pytest.raises(ValueError, match="go"):
        astchunk_chunk(content, go_chunk["file_path"], "go", max_chunk_size=300)


def test_astchunk_chunk_corpus_d25_fields():
    """All records from _astchunk_chunk_corpus carry the 9 required D25 fields."""
    corpus = load_corpus("auth_module")
    records = _astchunk_chunk_corpus(corpus, "python", max_chunk_size=300)
    required = {
        "symbol_name",
        "symbol_type",
        "file_path",
        "start_byte",
        "end_byte",
        "start_line",
        "end_line",
        "chunk_text",
        "language",
    }
    for i, rec in enumerate(records):
        missing = required - set(rec.keys())
        assert not missing, f"record[{i}] missing: {missing}"


def test_astchunk_chunk_corpus_json_serializable():
    """_astchunk_chunk_corpus records are JSON-serialisable (needed for dump_chunks)."""
    corpus = load_corpus("auth_module")
    records = _astchunk_chunk_corpus(corpus, "python", max_chunk_size=300)
    serialised = json.dumps(records)
    parsed = json.loads(serialised)
    assert len(parsed) == len(records)


# ---------------------------------------------------------------------------
# Scenario 2 — A/B row shape (integration, NEEDS BINARY)
# ---------------------------------------------------------------------------


def test_run_ab_astchunk_yields_two_rows(codecache_binary, tmp_path):
    """run_ab_astchunk returns exactly 2 rows: 'native' and 'astchunk'."""
    corpus = load_corpus("auth_module")
    rows = run_ab_astchunk(
        corpus=corpus,
        language="python",
        binary=codecache_binary,
        work_dir=tmp_path,
    )
    assert len(rows) == 2, f"expected 2 rows, got {len(rows)}"
    arms = {row["arm"] for row in rows}
    assert "native" in arms, f"missing 'native' arm; got {arms}"
    assert "astchunk" in arms, f"missing 'astchunk' arm; got {arms}"


def test_run_ab_astchunk_row_shape(codecache_binary, tmp_path):
    """Each row has arm, macro_all (with k=10), and n_queries keys."""
    corpus = load_corpus("auth_module")
    rows = run_ab_astchunk(
        corpus=corpus,
        language="python",
        binary=codecache_binary,
        work_dir=tmp_path,
    )
    for row in rows:
        assert "arm" in row
        assert "macro_all" in row
        assert "n_queries" in row
        m10 = row["macro_all"].get(10)
        assert m10 is not None, f"macro_all missing k=10 for arm={row['arm']}"
        assert hasattr(m10, "recall_file"), f"macro_all[10] has no recall_file for arm={row['arm']}"
        assert hasattr(m10, "ndcg_file"), f"macro_all[10] has no ndcg_file for arm={row['arm']}"


def test_run_ab_astchunk_no_winner_assertion(codecache_binary, tmp_path):
    """Outcome-agnostic: both arms have valid metrics; no winner is asserted here."""
    corpus = load_corpus("auth_module")
    rows = run_ab_astchunk(
        corpus=corpus,
        language="python",
        binary=codecache_binary,
        work_dir=tmp_path,
    )
    # Just verify metrics are in [0, 1] for both arms — no winner comparison
    for row in rows:
        m10 = row["macro_all"][10]
        assert 0.0 <= m10.ndcg_file <= 1.0, f"ndcg_file out of range for arm={row['arm']}"
        assert 0.0 <= m10.recall_file <= 1.0, f"recall_file out of range for arm={row['arm']}"
        assert 0.0 <= m10.f1_file <= 1.0, f"f1_file out of range for arm={row['arm']}"


def test_run_ab_astchunk_both_arms_same_query_count(codecache_binary, tmp_path):
    """Both arms score the same number of queries (identical gold)."""
    corpus = load_corpus("auth_module")
    rows = run_ab_astchunk(
        corpus=corpus,
        language="python",
        binary=codecache_binary,
        work_dir=tmp_path,
    )
    n_queries_set = {row["n_queries"] for row in rows}
    assert len(n_queries_set) == 1, (
        f"Arms scored different query counts — must use same gold: {[(r['arm'], r['n_queries']) for r in rows]}"
    )


def test_run_ab_astchunk_typescript(codecache_binary, tmp_path):
    """run_ab_astchunk works for the TypeScript config_module corpus."""
    corpus = load_corpus("config_module")
    rows = run_ab_astchunk(
        corpus=corpus,
        language="typescript",
        binary=codecache_binary,
        work_dir=tmp_path,
    )
    assert len(rows) == 2
    arms = {row["arm"] for row in rows}
    assert arms == {"native", "astchunk"}

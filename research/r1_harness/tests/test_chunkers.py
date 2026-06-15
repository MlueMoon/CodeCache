"""RED tests for R2.3b — stub chunker (r1harness.chunkers.stub_chunk).

Pure-logic, binary-free.  Covers:
  - happy path: Corpus → D25 records, one per gold chunk, all 9 required fields
  - offset truthfulness: byte/line offsets are materialize-consistent
  - valid D25 JSON shape (serialisation + default fields)
  - edge: empty chunk list → []
  - determinism: two calls yield byte-identical JSON

The production module (r1harness/chunkers.py) does NOT exist yet; every import
here will fail with ImportError — that is the correct RED state.

Offset contract (hand-computed, pinned):
  authenticate.py chunk 1 (authenticate_user):
    start_byte = 0
    end_byte   = 398   (len of chunk1_text.encode("utf-8"))
    start_line = 1
    end_line   = 9     (1 + 9 newlines - 1)

  authenticate.py chunk 2 (verify_password):
    start_byte = 398
    end_byte   = 587   (398 + 189)
    start_line = 10    (1 + 9 preceding newlines)
    end_line   = 13    (10 + 4 newlines - 1)

  Materialize-consistency invariant (for ALL chunks in the corpus):
    reconstructed_file[start_byte:end_byte] == chunk_text.encode("utf-8")
  where reconstructed_file = "".join(chunk_texts_for_file).encode("utf-8"),
  exactly as corpus.materialize() builds the on-disk file.
"""

from __future__ import annotations

import json

from r1harness.corpus import Corpus, load_corpus

# --- Production import (will fail RED: module does not exist yet) ---
from r1harness.chunkers import stub_chunk  # type: ignore[import]  # noqa: E402

# ---------------------------------------------------------------------------
# Helpers (inline — no production code)
# ---------------------------------------------------------------------------

D25_REQUIRED_FIELDS = {
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


def _reconstruct_file_bytes(corpus: Corpus, file_path: str) -> bytes:
    """Reconstruct the exact bytes materialize() would write for file_path."""
    chunks_for_file = [c["chunk_text"] for c in corpus.chunks if c["file_path"] == file_path]
    return "".join(chunks_for_file).encode("utf-8")


# ---------------------------------------------------------------------------
# Scenario 1 — happy path (pure, binary-free)
# ---------------------------------------------------------------------------


def test_stub_chunk_happy_path_count_and_required_fields():
    """stub_chunk returns one D25 record per gold chunk, all 9 required fields present."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    # record count == corpus chunk count
    assert len(records) == len(corpus.chunks), f"expected {len(corpus.chunks)} records, got {len(records)}"

    for i, rec in enumerate(records):
        missing = D25_REQUIRED_FIELDS - set(rec.keys())
        assert not missing, f"record[{i}] missing required fields: {missing}"


def test_stub_chunk_happy_path_enum_values():
    """symbol_type and language hold valid D25 enum strings."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    valid_symbol_types = {"function", "class", "method", "struct"}
    valid_languages = {"python", "typescript", "go"}

    for i, rec in enumerate(records):
        assert rec["symbol_type"] in valid_symbol_types, (
            f"record[{i}] symbol_type={rec['symbol_type']!r} not a valid D25 enum"
        )
        assert rec["language"] in valid_languages, f"record[{i}] language={rec['language']!r} not a valid D25 enum"


def test_stub_chunk_happy_path_matches_corpus_chunks():
    """Each record's symbol_name / file_path / chunk_text matches the source corpus chunk."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    for rec, src in zip(records, corpus.chunks, strict=True):
        assert rec["symbol_name"] == src["symbol_name"]
        assert rec["file_path"] == src["file_path"]
        assert rec["chunk_text"] == src["chunk_text"]


# ---------------------------------------------------------------------------
# Scenario 2 — offset truthfulness (THE critical test)
# ---------------------------------------------------------------------------

# Hand-computed expected values for authenticate.py (two chunks):
#   chunk1 = authenticate_user  (398 bytes, 9 newlines)
#   chunk2 = verify_password    (189 bytes, 4 newlines)
_CHUNK1_BYTE_LEN = 398
_CHUNK2_BYTE_LEN = 189


def test_offset_truthfulness_second_chunk_start_byte():
    """Second chunk's start_byte == UTF-8 byte length of first chunk's text."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    # Identify the two authenticate.py records (first two in fixture order)
    auth_records = [r for r in records if r["file_path"] == "src/auth/authenticate.py"]
    assert len(auth_records) == 2, "expected exactly 2 records for src/auth/authenticate.py"

    rec1, rec2 = auth_records[0], auth_records[1]
    assert rec1["symbol_name"] == "authenticate_user"
    assert rec2["symbol_name"] == "verify_password"

    # Pinned: chunk1 starts at byte 0
    assert rec1["start_byte"] == 0
    assert rec1["end_byte"] == _CHUNK1_BYTE_LEN

    # Pinned: chunk2 starts exactly where chunk1 ends
    assert rec2["start_byte"] == _CHUNK1_BYTE_LEN, (
        f"chunk2 start_byte={rec2['start_byte']}, expected {_CHUNK1_BYTE_LEN} (= len(chunk1_text.encode('utf-8')))"
    )
    assert rec2["end_byte"] == _CHUNK1_BYTE_LEN + _CHUNK2_BYTE_LEN, (
        f"chunk2 end_byte={rec2['end_byte']}, expected {_CHUNK1_BYTE_LEN + _CHUNK2_BYTE_LEN}"
    )


def test_offset_truthfulness_second_chunk_line_numbers():
    """Second chunk's start_line / end_line match hand-computed 1-based inclusive values."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    auth_records = [r for r in records if r["file_path"] == "src/auth/authenticate.py"]
    rec1, rec2 = auth_records[0], auth_records[1]

    # chunk1: authenticate_user — 9 newlines
    #   start_line = 1 (first chunk in file)
    #   end_line   = 1 + 9 - 1 = 9
    assert rec1["start_line"] == 1
    assert rec1["end_line"] == 9

    # chunk2: verify_password — 4 newlines
    #   start_line = 1 + 9 (preceding newlines) = 10
    #   end_line   = 10 + 4 - 1 = 13
    assert rec2["start_line"] == 10, (
        f"chunk2 start_line={rec2['start_line']}, expected 10 (= 1 + newlines in chunk1_text)"
    )
    assert rec2["end_line"] == 13, (
        f"chunk2 end_line={rec2['end_line']}, expected 13 (= start_line_2 + newlines_in_chunk2 - 1 = 10 + 4 - 1)"
    )


def test_offset_materialize_consistency_invariant():
    """reconstructed_file[start_byte:end_byte] == chunk_text.encode() for EVERY chunk."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    for rec in records:
        file_bytes = _reconstruct_file_bytes(corpus, rec["file_path"])
        sb = rec["start_byte"]
        eb = rec["end_byte"]
        expected = rec["chunk_text"].encode("utf-8")
        actual = file_bytes[sb:eb]
        assert actual == expected, (
            f"Invariant violated for {rec['file_path']}::{rec['symbol_name']}: "
            f"reconstructed_file[{sb}:{eb}] != chunk_text bytes. "
            f"Got {actual[:40]!r}, expected {expected[:40]!r}"
        )


# ---------------------------------------------------------------------------
# Scenario 3 — valid D25 JSON shape
# ---------------------------------------------------------------------------


def test_stub_chunk_json_is_top_level_array():
    """stub_chunk output serialises to a top-level JSON array."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)
    serialised = json.dumps(records)
    parsed = json.loads(serialised)
    assert isinstance(parsed, list)
    assert len(parsed) == len(records)


def test_stub_chunk_null_defaults_for_optional_scalar_fields():
    """parent_symbol and file_docstring default to null (absent in micro-suite records)."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)
    for i, rec in enumerate(records):
        # parent_symbol must be null / None (not absent, not a non-null string)
        assert "parent_symbol" in rec, f"record[{i}] missing parent_symbol key"
        assert rec["parent_symbol"] is None, f"record[{i}] parent_symbol={rec['parent_symbol']!r}, expected null/None"
        # file_docstring must be null / None
        assert "file_docstring" in rec, f"record[{i}] missing file_docstring key"
        assert rec["file_docstring"] is None, (
            f"record[{i}] file_docstring={rec['file_docstring']!r}, expected null/None"
        )


def test_stub_chunk_imports_and_cross_references_preserved():
    """imports and cross_references arrays are passed through from the corpus record."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)

    for rec, src in zip(records, corpus.chunks, strict=True):
        assert isinstance(rec["imports"], list), f"imports for {rec['symbol_name']} is not a list: {rec['imports']!r}"
        assert isinstance(rec["cross_references"], list), (
            f"cross_references for {rec['symbol_name']} is not a list: {rec['cross_references']!r}"
        )
        assert rec["imports"] == src.get("imports", []), f"imports mismatch for {rec['symbol_name']}"
        assert rec["cross_references"] == src.get("cross_references", []), (
            f"cross_references mismatch for {rec['symbol_name']}"
        )


def test_stub_chunk_is_heuristic_absent_or_false():
    """is_heuristic is absent or False (D25 default; storage drops it anyway)."""
    corpus = load_corpus("auth_module")
    records = stub_chunk(corpus)
    for i, rec in enumerate(records):
        val = rec.get("is_heuristic", False)
        assert val is False, f"record[{i}] is_heuristic={val!r}, expected absent or False"


# ---------------------------------------------------------------------------
# Scenario 4 — edge: empty corpus
# ---------------------------------------------------------------------------


def test_stub_chunk_empty_corpus_returns_empty_list():
    """stub_chunk over an empty chunk list emits [] (valid D25 no-op)."""
    empty_corpus = Corpus(id="empty", chunks=[])
    records = stub_chunk(empty_corpus)
    assert records == []


# ---------------------------------------------------------------------------
# Scenario 5 — determinism
# ---------------------------------------------------------------------------


def test_stub_chunk_is_deterministic():
    """Two calls with the same corpus produce byte-identical JSON output."""
    corpus = load_corpus("auth_module")
    first = json.dumps(stub_chunk(corpus), sort_keys=False)
    second = json.dumps(stub_chunk(corpus), sort_keys=False)
    assert first == second, "stub_chunk is not deterministic: two runs produced different JSON"


def test_stub_chunk_field_order_stable_across_runs():
    """Record field order is stable (same keys in same positions across calls)."""
    corpus = load_corpus("auth_module")
    records_a = stub_chunk(corpus)
    records_b = stub_chunk(corpus)
    for i, (ra, rb) in enumerate(zip(records_a, records_b, strict=True)):
        assert list(ra.keys()) == list(rb.keys()), f"record[{i}] key order differs between runs"

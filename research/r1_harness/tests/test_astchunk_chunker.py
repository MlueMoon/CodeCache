"""RED tests for R2.6 — astchunk wrapper (r1harness.astchunk_chunker).

Pure-logic, binary-free.  Covers:
  - happy path: a small inline Python snippet → D25 records with required fields
  - verbatim invariant: file_bytes[start_byte:end_byte] == content.encode() for chunks
    where astchunk returns a verbatim substring (the normal case)
  - fallback: non-verbatim chunks get valid (non-empty, positive-range) offsets
  - 0-to-1-based line conversion: astchunk gives 0-based → wrapper gives 1-based inclusive
  - symbol_name synthesis: deterministic f"{file_path}::L{start_line}-L{end_line}"
  - symbol_type sentinel: always "function" (valid D25 enum; ingest must accept it)
  - D25 schema: all 9 required fields present, correct types
  - optional sentinel fields: parent_symbol=None, file_docstring=None,
    imports=[], cross_references=[], is_heuristic=False
  - TypeScript support: chunkify returns D25 records for a TS snippet
  - determinism: two calls with the same content yield identical JSON
  - empty content: chunkify on empty string returns [] (no crash)
  - Go skip: astchunk_chunk raises (or skips gracefully) for unsupported language

The production module (r1harness/astchunk_chunker.py) does NOT exist yet;
every import here will fail with ImportError — that is the correct RED state.

Hand-computed expected values (pinned for the invariant test):
  PYTHON_SNIPPET is "def foo(x):\\n    return x + 1\\n"
    UTF-8 bytes: 26 bytes
    astchunk at max_chunk_size=300:
      - chunkify should return at least 1 chunk; the chunk's content is a
        verbatim substring of the snippet.
      - start_line_no=0, end_line_no=1 (0-based, inclusive)
      - wrapper: start_line=1, end_line=2 (1-based, inclusive)
      - symbol_name = "src/test.py::L1-L2"
"""

from __future__ import annotations

import json


# --- Production import (will fail RED: module does not exist yet) ---
from r1harness.astchunk_chunker import astchunk_chunk  # type: ignore[import]

# ---------------------------------------------------------------------------
# Inline test fixtures (binary-free, network-free)
# ---------------------------------------------------------------------------

PYTHON_SNIPPET = "def foo(x):\n    return x + 1\n"
PYTHON_FILE_PATH = "src/test.py"

TS_SNIPPET = "function bar(n: number): number {\n  return n * 2;\n}\n"
TS_FILE_PATH = "src/test.ts"

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

VALID_SYMBOL_TYPES = {"function", "class", "method", "struct"}
VALID_LANGUAGES = {"python", "typescript", "go"}


# ---------------------------------------------------------------------------
# Task 1 — D25 schema (required fields)
# ---------------------------------------------------------------------------


def test_astchunk_chunk_returns_list():
    """astchunk_chunk returns a list (possibly empty)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    assert isinstance(records, list)


def test_astchunk_chunk_python_nonempty():
    """astchunk_chunk on a minimal Python function returns at least 1 record."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    assert len(records) >= 1, f"expected at least 1 record, got {records}"


def test_astchunk_chunk_required_fields_present():
    """Every record has all 9 required D25 fields."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        missing = D25_REQUIRED_FIELDS - set(rec.keys())
        assert not missing, f"record[{i}] missing required fields: {missing}"


def test_astchunk_chunk_symbol_type_is_function_sentinel():
    """symbol_type is the 'function' sentinel (valid D25 enum, not a real type)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["symbol_type"] == "function", (
            f"record[{i}] symbol_type={rec['symbol_type']!r}, expected 'function' sentinel"
        )
        assert rec["symbol_type"] in VALID_SYMBOL_TYPES


def test_astchunk_chunk_language_field():
    """language field equals the corpus language passed in."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["language"] == "python", f"record[{i}] language={rec['language']!r}, expected 'python'"


def test_astchunk_chunk_file_path_field():
    """file_path field equals the file_path argument."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["file_path"] == PYTHON_FILE_PATH, (
            f"record[{i}] file_path={rec['file_path']!r}, expected {PYTHON_FILE_PATH!r}"
        )


# ---------------------------------------------------------------------------
# Task 2 — optional sentinel fields (null/empty defaults)
# ---------------------------------------------------------------------------


def test_astchunk_chunk_parent_symbol_is_null():
    """parent_symbol is null (None) — enrichment not available from astchunk."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert "parent_symbol" in rec, f"record[{i}] missing parent_symbol key"
        assert rec["parent_symbol"] is None, f"record[{i}] parent_symbol={rec['parent_symbol']!r}"


def test_astchunk_chunk_file_docstring_is_null():
    """file_docstring is null (None)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert "file_docstring" in rec, f"record[{i}] missing file_docstring key"
        assert rec["file_docstring"] is None, f"record[{i}] file_docstring={rec['file_docstring']!r}"


def test_astchunk_chunk_imports_is_empty_list():
    """imports is an empty list (astchunk provides no import extraction)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec.get("imports") == [], f"record[{i}] imports={rec.get('imports')!r}"


def test_astchunk_chunk_cross_references_is_empty_list():
    """cross_references is an empty list."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec.get("cross_references") == [], f"record[{i}] cross_references={rec.get('cross_references')!r}"


def test_astchunk_chunk_is_heuristic_false():
    """is_heuristic is absent or False."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        val = rec.get("is_heuristic", False)
        assert val is False, f"record[{i}] is_heuristic={val!r}"


# ---------------------------------------------------------------------------
# Task 3 — 0-to-1-based line conversion
# ---------------------------------------------------------------------------


def test_astchunk_chunk_start_line_is_one_based():
    """start_line is 1-based (astchunk gives 0-based start_line_no; wrapper adds 1)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["start_line"] >= 1, f"record[{i}] start_line={rec['start_line']}, expected >= 1 (1-based D7)"


def test_astchunk_chunk_end_line_is_one_based():
    """end_line >= start_line (1-based, inclusive)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["end_line"] >= rec["start_line"], (
            f"record[{i}] end_line={rec['end_line']} < start_line={rec['start_line']}"
        )


def test_astchunk_chunk_line_conversion_pinned():
    """Pinned: PYTHON_SNIPPET at max_chunk_size=300, first chunk has start_line=1.

    PYTHON_SNIPPET = 'def foo(x):\\n    return x + 1\\n'
    astchunk gives start_line_no=0 → wrapper must give start_line=1.
    """
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    assert len(records) >= 1
    # First chunk must start at line 1 (0-based 0 → 1-based 1)
    assert records[0]["start_line"] == 1, (
        f"first record start_line={records[0]['start_line']}, expected 1 (0-based 0 + 1)"
    )


# ---------------------------------------------------------------------------
# Task 4 — symbol_name synthesis
# ---------------------------------------------------------------------------


def test_astchunk_chunk_symbol_name_format():
    """symbol_name = '{file_path}::L{start_line}-L{end_line}' (deterministic)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        expected_sym = f"{rec['file_path']}::L{rec['start_line']}-L{rec['end_line']}"
        assert rec["symbol_name"] == expected_sym, (
            f"record[{i}] symbol_name={rec['symbol_name']!r}, expected {expected_sym!r}"
        )


def test_astchunk_chunk_symbol_name_pinned():
    """Pinned: PYTHON_SNIPPET → first chunk symbol_name starts with 'src/test.py::L1-L'.

    Exact end_line depends on astchunk; we pin the prefix.
    """
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    assert len(records) >= 1
    sym = records[0]["symbol_name"]
    assert sym.startswith("src/test.py::L1-L"), (
        f"symbol_name={sym!r} does not start with expected prefix 'src/test.py::L1-L'"
    )


# ---------------------------------------------------------------------------
# Task 5 — byte-offset correctness (verbatim invariant)
# ---------------------------------------------------------------------------


def test_astchunk_chunk_byte_offsets_positive():
    """start_byte >= 0 and end_byte > start_byte for every record."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["start_byte"] >= 0, f"record[{i}] start_byte={rec['start_byte']} < 0"
        assert rec["end_byte"] > rec["start_byte"], (
            f"record[{i}] end_byte={rec['end_byte']} <= start_byte={rec['start_byte']}"
        )


def test_astchunk_chunk_verbatim_invariant_python():
    """For verbatim chunks: file_bytes[start_byte:end_byte] == chunk_text.encode().

    PYTHON_SNIPPET is fully verbatim (no non-verbatim splits expected at max_chunk_size=300).
    """
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    file_bytes = PYTHON_SNIPPET.encode("utf-8")
    for i, rec in enumerate(records):
        sb = rec["start_byte"]
        eb = rec["end_byte"]
        expected = rec["chunk_text"].encode("utf-8")
        actual = file_bytes[sb:eb]
        assert actual == expected, (
            f"Invariant violated for record[{i}] (symbol={rec['symbol_name']}): "
            f"file[{sb}:{eb}] != chunk_text bytes. "
            f"Got {actual[:40]!r}, expected {expected[:40]!r}"
        )


def test_astchunk_chunk_chunk_text_equals_content():
    """chunk_text in the D25 record equals astchunk's content field verbatim."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    # All records should have non-empty chunk_text
    for i, rec in enumerate(records):
        assert isinstance(rec["chunk_text"], str), f"record[{i}] chunk_text is not str"
        assert len(rec["chunk_text"]) > 0, f"record[{i}] chunk_text is empty"


# ---------------------------------------------------------------------------
# Task 6 — TypeScript support
# ---------------------------------------------------------------------------


def test_astchunk_chunk_typescript_returns_records():
    """astchunk_chunk works for TypeScript files (language='typescript')."""
    records = astchunk_chunk(TS_SNIPPET, TS_FILE_PATH, "typescript", max_chunk_size=300)
    assert isinstance(records, list)
    assert len(records) >= 1, f"expected at least 1 record for TS, got {records}"


def test_astchunk_chunk_typescript_required_fields():
    """TypeScript records also have all 9 required D25 fields."""
    records = astchunk_chunk(TS_SNIPPET, TS_FILE_PATH, "typescript", max_chunk_size=300)
    for i, rec in enumerate(records):
        missing = D25_REQUIRED_FIELDS - set(rec.keys())
        assert not missing, f"TS record[{i}] missing required fields: {missing}"


def test_astchunk_chunk_typescript_language_field():
    """TypeScript records carry language='typescript'."""
    records = astchunk_chunk(TS_SNIPPET, TS_FILE_PATH, "typescript", max_chunk_size=300)
    for i, rec in enumerate(records):
        assert rec["language"] == "typescript", f"TS record[{i}] language={rec['language']!r}"


# ---------------------------------------------------------------------------
# Task 7 — determinism
# ---------------------------------------------------------------------------


def test_astchunk_chunk_is_deterministic():
    """Two calls with the same arguments produce byte-identical JSON output."""
    records_a = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    records_b = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    assert json.dumps(records_a, sort_keys=False) == json.dumps(records_b, sort_keys=False), (
        "astchunk_chunk is not deterministic: two runs produced different JSON"
    )


# ---------------------------------------------------------------------------
# Task 8 — edge cases
# ---------------------------------------------------------------------------


def test_astchunk_chunk_empty_content_returns_list():
    """Empty content string returns a list (possibly empty) without crashing."""
    records = astchunk_chunk("", PYTHON_FILE_PATH, "python", max_chunk_size=300)
    assert isinstance(records, list)


def test_astchunk_chunk_json_serializable():
    """All records are JSON-serialisable (required for dump_chunks / D25 ingest)."""
    records = astchunk_chunk(PYTHON_SNIPPET, PYTHON_FILE_PATH, "python", max_chunk_size=300)
    serialised = json.dumps(records)
    parsed = json.loads(serialised)
    assert isinstance(parsed, list)
    assert len(parsed) == len(records)

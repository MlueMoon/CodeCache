"""astchunk wrapper for R2.6: turns a materialised source file into D25 ingest records.

This is the cAST baseline chunker, replacing the R2.3b stub chunker in the A/B
plumbing.  It uses the ``astchunk`` PyPI package (MIT, 0.1.0) to do AST-aware
chunking via Tree-sitter.  The only variable vs the native arm is the chunker:
storage, FTS5, retriever, and enrichment are all held constant.

Design decisions
----------------
``max_chunk_size``
    astchunk's non-whitespace character budget per chunk.  Set to **300** — this
    keeps chunks comparable in size to the micro-suite gold chunks (~160–400 chars
    non-whitespace) while avoiding over-fragmentation.  It is a hyperparameter;
    changing it changes the chunking boundary, not the scoring protocol.

``symbol_name`` synthesis
    astchunk does not emit symbol names.  We synthesise a deterministic id:
    ``"{file_path}::L{start_line}-L{end_line}"`` (1-based inclusive lines).
    This matches the **file-level scoring** decision documented in the R2.6 task:
    since retrieved block keys (file_path, symbol_name) will never match gold
    block keys (which carry real function names), block-level metrics are ~0 and
    are NOT reported as the headline; file-level metrics ARE reported.

``symbol_type`` sentinel
    Set to ``"function"`` — a valid D25 enum value that the ``codecache ingest``
    command accepts.  It is NOT a real symbol type; it is a placeholder required
    by the schema.  Documented here and in the report.

Byte-offset contract
--------------------
For each chunk, we first try to locate ``content.encode("utf-8")`` as a
**verbatim substring** of the file bytes (using a monotonically advancing cursor
to handle repeated text).  When the search succeeds:
    start_byte = found_position
    end_byte   = found_position + len(content_bytes)
and the invariant ``file_bytes[start_byte:end_byte] == content.encode()`` holds.

When astchunk returns a non-verbatim chunk (e.g. a function-body fragment that
starts mid-line), we fall back to **line-number-derived** offsets:
    start_byte = sum of UTF-8 byte lengths of all preceding lines
    end_byte   = start_byte + len(content_bytes)
In this case the strict invariant does NOT hold (documented limitation); the
byte range still points to a valid non-empty region of the file and will not
cause the ingest command to reject the record.

Language support
----------------
astchunk 0.1.0 supports Python and TypeScript.  Go is NOT supported.
Callers must not pass ``language="go"``; :func:`astchunk_chunk` will raise
``ValueError`` for unsupported languages.
"""

from __future__ import annotations

_SUPPORTED_LANGUAGES = {"python", "typescript"}


def _line_start_bytes(file_bytes: bytes) -> list[int]:
    """Return a list where index i is the byte offset of line i (0-based)."""
    offsets = [0]
    pos = 0
    while True:
        nl = file_bytes.find(b"\n", pos)
        if nl == -1:
            break
        pos = nl + 1
        offsets.append(pos)
    return offsets


def astchunk_chunk(
    content: str,
    file_path: str,
    language: str,
    *,
    max_chunk_size: int = 300,
) -> list[dict]:
    """Chunk a source file's content using astchunk and return D25 ingest records.

    Args:
        content:        Full UTF-8 text of the source file (as read from disk).
        file_path:      Repo-relative path for this file (used in D25 ``file_path``
                        field and ``symbol_name`` synthesis).
        language:       Corpus language — ``"python"`` or ``"typescript"``.
                        Raises ``ValueError`` for unsupported languages (e.g. ``"go"``).
        max_chunk_size: Non-whitespace character budget per chunk (astchunk param).
                        Default: 300 (see module docstring for rationale).

    Returns:
        A list of D25 ingest record dicts, one per astchunk chunk.  Each record
        carries all required D25 fields plus the optional sentinel/default fields.
        Returns an empty list if ``content`` is empty or astchunk emits no chunks.

    Raises:
        ValueError: if ``language`` is not in the supported set
                    (``{"python", "typescript"}``).
    """
    if language not in _SUPPORTED_LANGUAGES:
        raise ValueError(
            f"astchunk_chunk: language={language!r} is not supported by astchunk 0.1.0. "
            f"Supported: {sorted(_SUPPORTED_LANGUAGES)}.  Skip Go corpora at the caller."
        )

    if not content:
        return []

    from astchunk import ASTChunkBuilder  # local import: optional dep

    builder = ASTChunkBuilder(
        max_chunk_size=max_chunk_size,
        language=language,
        metadata_template="default",
    )
    raw_chunks = builder.chunkify(content, repo_level_metadata={"filepath": file_path})

    if not raw_chunks:
        return []

    file_bytes = content.encode("utf-8")
    line_starts = _line_start_bytes(file_bytes)

    records: list[dict] = []
    cursor = 0  # monotonically advancing cursor for verbatim search

    for chunk in raw_chunks:
        chunk_text: str = chunk["content"]
        meta: dict = chunk["metadata"]

        chunk_bytes = chunk_text.encode("utf-8")

        # --- Byte offsets ---
        pos = file_bytes.find(chunk_bytes, cursor)
        if pos >= 0:
            # Verbatim case: invariant file_bytes[sb:eb] == chunk_bytes holds.
            start_byte = pos
            end_byte = pos + len(chunk_bytes)
        else:
            # Non-verbatim fallback: body-only fragment starting mid-line.
            # Use the line-number-derived start byte as the best approximation.
            # The strict invariant does NOT hold in this case (documented limitation).
            start_line_no: int = meta["start_line_no"]  # 0-based
            if start_line_no < len(line_starts):
                start_byte = line_starts[start_line_no]
            else:
                start_byte = len(file_bytes)
            end_byte = start_byte + len(chunk_bytes)

        # Advance cursor to end of verbatim match (or to eb for fallback).
        cursor = end_byte

        # --- 0-based → 1-based inclusive line numbers (D7) ---
        start_line: int = meta["start_line_no"] + 1
        end_line: int = meta["end_line_no"] + 1

        # --- Synthesised symbol_name (deterministic) ---
        symbol_name: str = f"{file_path}::L{start_line}-L{end_line}"

        record: dict = {
            "symbol_name": symbol_name,
            # "function" is a valid D25 enum sentinel; NOT a real symbol type.
            # astchunk provides no symbol-kind information.
            "symbol_type": "function",
            "file_path": file_path,
            "start_byte": start_byte,
            "end_byte": end_byte,
            "start_line": start_line,
            "end_line": end_line,
            "chunk_text": chunk_text,
            "language": language,
            # Enrichment fields: all defaults (astchunk provides no enrichment).
            "parent_symbol": None,
            "file_docstring": None,
            "imports": [],
            "cross_references": [],
            "is_heuristic": False,
        }
        records.append(record)

    return records

"""Stub chunker for R2.3b: turns a Corpus into valid D25 ingest records.

The stub chunker is the simplest swappable chunker (gold chunk boundaries → D25 records).
astchunk/cAST is the gated R2.6 replacement that drops into the SAME plumbing — R2.3b's job
is to prove the plumbing.

Offset contract (D25 / brief §"The offset-synthesis contract"):
- start_byte[i] = sum(len(c["chunk_text"].encode("utf-8")) for preceding same-file chunks)
- end_byte[i]   = start_byte[i] + len(chunk_text.encode("utf-8"))   (half-open [start, end))
- start_line[i] = 1 + sum(c["chunk_text"].count("\\n") for preceding same-file chunks)
- end_line[i]   = start_line[i] + chunk_text.count("\\n") - 1

Offsets point into the exact bytes that corpus.materialize() writes:
  "".join(chunk_texts_for_file).encode("utf-8")
so reconstructed_file[start_byte:end_byte] == chunk_text.encode("utf-8") for every record.
"""

from __future__ import annotations

import json
from pathlib import Path

from .corpus import Corpus


def stub_chunk(corpus: Corpus) -> list[dict]:
    """Convert a Corpus into a list of D25 ingest records with truthful offsets.

    One record per gold chunk, in fixture order.  Byte/line offsets are synthesised
    so they are materialize-consistent (the reconstructed file slice equals chunk_text).

    Args:
        corpus: a Corpus loaded from the micro-suite fixture.

    Returns:
        A list of D25 ingest record dicts (serialisable to JSON array).
    """
    # Per-file running counters: byte offset and newline count.
    file_byte_offset: dict[str, int] = {}
    file_newline_count: dict[str, int] = {}

    records: list[dict] = []
    for chunk in corpus.chunks:
        fp = chunk["file_path"]

        # Initialise per-file counters on first encounter (fixture order = materialize order).
        if fp not in file_byte_offset:
            file_byte_offset[fp] = 0
            file_newline_count[fp] = 0

        text: str = chunk["chunk_text"]
        text_bytes: bytes = text.encode("utf-8")
        n_newlines: int = text.count("\n")

        start_byte: int = file_byte_offset[fp]
        end_byte: int = start_byte + len(text_bytes)
        start_line: int = 1 + file_newline_count[fp]
        end_line: int = start_line + n_newlines - 1

        record: dict = {
            "symbol_name": chunk["symbol_name"],
            "symbol_type": chunk["symbol_type"],
            "file_path": fp,
            "start_byte": start_byte,
            "end_byte": end_byte,
            "start_line": start_line,
            "end_line": end_line,
            "chunk_text": text,
            "language": chunk["language"],
            "imports": list(chunk.get("imports", [])),
            "cross_references": list(chunk.get("cross_references", [])),
            "parent_symbol": None,
            "file_docstring": None,
            "is_heuristic": False,
        }
        records.append(record)

        # Advance per-file counters.
        file_byte_offset[fp] = end_byte
        file_newline_count[fp] += n_newlines

    return records


def dump_chunks(records: list[dict], path: Path) -> None:
    """Serialise D25 ingest records to a JSON file (top-level array).

    Args:
        records: list of D25 record dicts produced by stub_chunk().
        path: destination file path (written with UTF-8 encoding).
    """
    Path(path).write_text(json.dumps(records, ensure_ascii=False), encoding="utf-8")

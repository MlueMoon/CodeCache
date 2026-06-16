"""Run the R2.6 astchunk A/B comparison against the real codecache binary (zero-cost, local).

For each SUPPORTED micro-suite corpus (Python + TypeScript only), materialises the repo tree
and runs two arms:
  - **native**:   init → index → query   (CodeCache's built-in Tree-sitter chunker)
  - **astchunk**: init → ingest <astchunk_chunks.json> → query  (astchunk cAST chunker)

Both arms are scored by the existing Layer-1/NDCG scorer against the SAME gold.

Go corpus (``data_processing``) is SKIPPED — astchunk 0.1.0 has no Go grammar.

Headline metrics: FILE-LEVEL (ndcg_file, f1_file, recall_file @ k=10).
Block-level is included in the JSON output with a caveat: synthesised symbol names
(``"{file_path}::L{start_line}-L{end_line}"``) never match real gold block keys, so
block-level metrics for the astchunk arm will be ~0 and should not be interpreted as
the headline comparison.

max_chunk_size=300 (astchunk hyperparameter; non-whitespace chars per chunk).

Does NOT assert a winner (outcome-agnostic, project_overview §7).

Run (from research/r1_harness/, debug or release binary built):
    PYTHONUTF8=1 .venv/bin/python run_ab_astchunk.py
"""

from __future__ import annotations

import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

HERE = Path(__file__).resolve().parent

# Corpora supported by astchunk 0.1.0 (Python + TypeScript only; Go has no grammar).
_CORPUS_LANGUAGE: dict[str, str] = {
    "auth_module": "python",
    "config_module": "typescript",
}

_SKIPPED_CORPORA = {"data_processing"}
_MAX_CHUNK_SIZE = 300


def main() -> int:
    from r1harness.ab_runner import run_ab_astchunk
    from r1harness.codecache_tool import find_codecache_binary
    from r1harness.corpus import load_corpus
    from r1harness.sweep import load_suite

    binary = find_codecache_binary()
    all_queries = load_suite()
    corpus_ids = sorted({q.corpus_id for q in all_queries})

    runs_dir = HERE / "runs" / "ab_astchunk"
    runs_dir.mkdir(parents=True, exist_ok=True)

    print(f"=== R2.6 astchunk A/B comparison — binary={binary.name} max_chunk_size={_MAX_CHUNK_SIZE} ===")
    print(f"  Supported corpora: {sorted(_CORPUS_LANGUAGE)}")
    print(f"  Skipped (no Go grammar in astchunk 0.1.0): {sorted(_SKIPPED_CORPORA)}")
    print()

    all_rows: list[dict] = []

    for cid in corpus_ids:
        if cid in _SKIPPED_CORPORA:
            print(f"  corpus: {cid}  [SKIPPED — astchunk has no Go grammar]")
            print()
            continue

        language = _CORPUS_LANGUAGE[cid]
        corpus = load_corpus(cid)
        work_dir = runs_dir / cid
        work_dir.mkdir(parents=True, exist_ok=True)

        print(f"  corpus: {cid}  (language={language})")
        rows = run_ab_astchunk(
            corpus=corpus,
            language=language,
            binary=binary,
            work_dir=work_dir,
            max_chunk_size=_MAX_CHUNK_SIZE,
        )

        for row in rows:
            m10 = row["macro_all"][10]
            print(
                f"    arm={row['arm']:10s}  n_queries={row['n_queries']}  "
                f"ndcg_file@10={m10.ndcg_file:.3f}  "
                f"f1_file@10={m10.f1_file:.3f}  "
                f"recall_file@10={m10.recall_file:.3f}"
            )
            serialised_row = {
                "corpus_id": cid,
                "arm": row["arm"],
                "n_queries": row["n_queries"],
                "macro_all": {str(k): asdict(m) for k, m in row["macro_all"].items()},
            }
            all_rows.append(serialised_row)
        print()

    report = {
        "binary": str(binary),
        "max_chunk_size": _MAX_CHUNK_SIZE,
        "skipped_corpora": sorted(_SKIPPED_CORPORA),
        "note": (
            "File-level metrics are the R2.6 headline (ndcg_file/f1_file/recall_file). "
            "Block-level metrics for the astchunk arm are ~0 because synthesised symbol names "
            "('file::L{start}-L{end}') do not match real gold block keys — do not use as headline. "
            "This is a directional PROXY (15-query micro-suite, Python+TS only, n=10 queries). "
            "No arm winner is asserted (outcome-agnostic, project_overview §7)."
        ),
        "rows": all_rows,
    }
    report_path = runs_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    print(f"report: {report_path}")
    print(
        "(Outcome-agnostic: no arm winner asserted — "
        "directional PROXY on 15-query micro-suite, Python+TS only. R3 gates the final determination.)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

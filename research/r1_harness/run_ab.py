"""Run the R2.3b A/B chunker comparison against the real codecache binary (zero-cost, local).

For each micro-suite corpus, materialises the repo tree and runs two arms:
  - **native**: init → index → query  (CodeCache's built-in Tree-sitter chunker)
  - **stub**:   init → ingest <stub.json> → query  (stub_chunk via the D25 ingest seam)

Both arms are scored by the existing Layer-1/NDCG scorer against the SAME gold.
Prints a minimal summary and writes ``runs/ab/report.json``.

Does NOT assert a winner (outcome-agnostic, project_overview §7).

Run (from research/r1_harness/, debug or release binary built):
    PYTHONUTF8=1 C:/ccr1/Scripts/python.exe run_ab.py
"""

from __future__ import annotations

import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

HERE = Path(__file__).resolve().parent


def main() -> int:
    from r1harness.ab_runner import run_ab
    from r1harness.codecache_tool import find_codecache_binary
    from r1harness.corpus import load_corpus
    from r1harness.sweep import load_suite

    binary = find_codecache_binary()
    all_queries = load_suite()
    corpus_ids = sorted({q.corpus_id for q in all_queries})

    runs_dir = HERE / "runs" / "ab"
    runs_dir.mkdir(parents=True, exist_ok=True)

    print(f"=== R2.3b A/B chunker comparison — binary={binary.name} ===")
    print(f"{len(corpus_ids)} corpus/corpora; native arm vs stub arm")
    print()

    all_rows: list[dict] = []
    for cid in corpus_ids:
        corpus = load_corpus(cid)
        work_dir = runs_dir / cid
        work_dir.mkdir(parents=True, exist_ok=True)

        print(f"  corpus: {cid}")
        rows = run_ab(corpus=corpus, binary=binary, work_dir=work_dir)

        for row in rows:
            m10 = row["macro_all"][10]
            print(
                f"    arm={row['arm']:8s}  n_queries={row['n_queries']}  "
                f"recall_block@10={m10.recall_block:.3f}  "
                f"ndcg_block@10={m10.ndcg_block:.3f}"
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
        "rows": all_rows,
    }
    report_path = runs_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    print(f"report: {report_path}")
    print("(Outcome-agnostic: no arm winner asserted — R3 gates the final determination.)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Live R1 end-to-end run — A0/A1/A4 against a LOCAL model via Ollama (zero-cost).

Same pipeline as ``validate_offline.py``, but swaps mini's ``DeterministicModel`` for a
real litellm-backed local model (default ``ollama_chat/qwen2.5:7b``). The model runs
entirely on a local Ollama server — no API key, no paid spend — so this realises R1's
*live* exit criterion (D22) while honouring the project's zero-cost, local-first rule.
It lets us observe how a real LM's own choices (which commands it runs, how many turns it
takes, whether it follows the submit protocol) differ from the scripted deterministic
baseline.

This is apparatus + observation, **not** an arm-winner claim — that is an R3
determination over a real corpus with replication (project_overview §7).

Prereqs:
  * Ollama running on :11434 with the model pulled — ``ollama pull qwen2.5:7b``.
  * The short-path venv that has mini-swe-agent (docs/TESTING_AND_USAGE.md §3).
  * The release binary built (``cargo build --release``) or ``$CODECACHE_BIN`` set.

Run (from research/r1_harness/):
    PYTHONUTF8=1 C:/ccr1/Scripts/python.exe run_live.py
    PYTHONUTF8=1 C:/ccr1/Scripts/python.exe run_live.py \
        --model ollama_chat/llama3 --model-class litellm_textbased --steps 10

``qwen2.5:7b`` advertises native tool-calling, so it uses the default ``litellm`` class
(``tools=[BASH_TOOL]``); models without tool support (llama3/phi3) need
``--model-class litellm_textbased`` (the model writes a ```` ```mswea_bash_command ````
block that mini parses from text).
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

HERE = Path(__file__).resolve().parent


def build_live_factory(model_name: str, model_class: str, temperature: float):
    """Return a ``factory(_outputs)`` that builds a local litellm model (Ollama).

    ``cost_tracking='ignore_errors'`` is required: local models have no litellm cost
    table, and the default behaviour raises rather than returning a 0 cost — which would
    abort the run on the first model call.
    """
    from minisweagent.models.litellm_model import LitellmModel
    from minisweagent.models.litellm_textbased_model import LitellmTextbasedModel

    classes = {"litellm": LitellmModel, "litellm_textbased": LitellmTextbasedModel}
    cls = classes[model_class]

    def factory(_outputs):  # live: the model decides; scripted outputs are ignored
        return cls(
            model_name=model_name,
            model_kwargs={"temperature": temperature},
            cost_tracking="ignore_errors",
        )

    return factory


_HDR = (
    f"{'arm':4} {'R@1 file':>9} {'R@1 blk':>8} {'cov file':>8} {'cov blk':>8} "
    f"{'F1@10 blk':>10} {'turns→cov':>10} {'tok→cov':>9} {'tot tok':>8}"
)


def _fmt_arm_row(name: str, a: dict) -> str:
    if "error" in a:
        return f"{name:4} {'ERROR':>9}  {a['error'][:64]}"
    at1, at10, l2 = a["layer1"]["@1"], a["layer1"]["@10"], a["layer2"]
    t2c = a["layer2"]["turns_to_coverage"]
    k2c = a["layer2"]["tokens_to_coverage"]
    return (
        f"{name:4} {at1['recall_file']:9.2f} {at1['recall_block']:8.2f} "
        f"{at10['recall_file']:8.2f} {at10['recall_block']:8.2f} {at10['f1_block']:10.2f} "
        f"{str(t2c):>10} {str(k2c):>9} {l2['total_tokens']:8d}"
    )


def main() -> int:
    ap = argparse.ArgumentParser(description="Live R1 A0/A1/A4 run against a local Ollama model.")
    ap.add_argument("--model", default="ollama_chat/qwen2.5:7b", help="litellm model id (Ollama).")
    ap.add_argument(
        "--model-class", default="litellm", choices=["litellm", "litellm_textbased"],
        help="litellm = native tool-calling (qwen2.5); litellm_textbased = bash-block parsing (llama3/phi3).",
    )
    ap.add_argument("--steps", type=int, default=8, help="per-arm step budget (bounds the live loop).")
    ap.add_argument("--wall", type=int, default=600, help="per-arm wall-clock limit (seconds).")
    ap.add_argument("--temperature", type=float, default=0.0)
    args = ap.parse_args()

    os.environ.setdefault("MSWEA_COST_TRACKING", "ignore_errors")
    os.environ.setdefault("MSWEA_SILENT_STARTUP", "1")
    os.environ.setdefault("OLLAMA_API_BASE", "http://localhost:11434")

    from r1harness.arms import R1_ARMS, Task
    from r1harness.codecache_tool import find_codecache_binary
    from r1harness.runner import TEXTBASED_PROTOCOL, TOOLCALL_PROTOCOL, run_all

    task = Task.from_dict(json.loads((HERE / "tasks" / "auth_q1.json").read_text(encoding="utf-8")))
    runs_dir = HERE / "runs" / "live"
    binary = find_codecache_binary()
    arms = [R1_ARMS["A0"], R1_ARMS["A1"], R1_ARMS["A4"]]

    print(f"=== R1 LIVE run — model={args.model} ({args.model_class}), temp={args.temperature} ===")
    print(f"task {task.task_id!r}: {task.query!r}")
    print(f"gold file = {sorted(task.gold_files)}, gold block = {sorted(task.gold_blocks)}")
    print(f"binary    = {binary}")
    print("running A0/A1/A4 (first model call loads the model into Ollama — expect ~1 min)...\n")

    protocol = TEXTBASED_PROTOCOL if args.model_class == "litellm_textbased" else TOOLCALL_PROTOCOL
    factory = build_live_factory(args.model, args.model_class, args.temperature)
    report = run_all(
        task, arms, runs_dir, factory, binary=binary,
        step_limit=args.steps, wall_time_limit_seconds=args.wall,
        model_label=args.model, temperature=args.temperature,
        action_protocol=protocol, continue_on_error=True,
    )
    report["model"] = args.model
    report["model_class"] = args.model_class
    report["temperature"] = args.temperature
    runs_dir.mkdir(parents=True, exist_ok=True)
    (runs_dir / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")

    print("LIVE results (cov = recall@10 — share of gold surfaced anywhere in the run):")
    print(_HDR)
    for name in ("A0", "A1", "A4"):
        print(_fmt_arm_row(name, report["arms"][name]))

    # Side-by-side with the deterministic baseline, if validate_offline.py has been run.
    offline_path = HERE / "runs" / "report.json"
    if offline_path.exists():
        off = json.loads(offline_path.read_text(encoding="utf-8"))
        print("\nDeterministic baseline (validate_offline.py), same columns:")
        print(_HDR)
        for name in ("A0", "A1", "A4"):
            if name in off.get("arms", {}):
                print(_fmt_arm_row(name, off["arms"][name]))

    print(f"\nreport:       {runs_dir / 'report.json'}")
    print("trajectories: runs/live/<arm>/trajectory.jsonl  (+ mini_trajectory.json = full message log)")
    covered = [
        n for n in ("A0", "A1", "A4")
        if "error" not in report["arms"][n]
        and report["arms"][n]["layer1"]["@10"]["recall_block"] >= 1.0
    ]
    errored = [n for n in ("A0", "A1", "A4") if "error" in report["arms"][n]]
    print(f"\ngold block covered by: {covered or 'none'}"
          + (f"   |   arm errors: {errored}" if errored else ""))
    print("(Observation of one live run — NOT an arm-winner claim; that is R3.)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

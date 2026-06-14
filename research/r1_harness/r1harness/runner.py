"""Run one task through an arm with mini-SWE-agent, logging a scored trajectory.

`LoggingAgent` subclasses mini's `DefaultAgent` to capture, per turn, the
gold-comparable items each action surfaced (via :mod:`r1harness.extract`) into a
JSONL trajectory (:mod:`r1harness.trajectory`). `run_arm` materialises the
corpus, wires the arm's retrieval surface, drives the agent, and returns the
trajectory path; `run_all` scores every arm's trajectory and assembles the
Layer-1/Layer-2 metrics report (project_overview §5.2).

The model is injected by the caller — for offline validation it is mini's
`DeterministicModel` (scripted actions, no API, no cost). A live model
(litellm) is a drop-in replacement gated on a backend choice (D22).
"""

from __future__ import annotations

from dataclasses import asdict
from pathlib import Path

from minisweagent.agents.default import DefaultAgent
from minisweagent.exceptions import Submitted

from .arms import Arm, Task
from .bash_env import BashEnvironment
from .codecache_tool import CodeCacheIndex, find_codecache_binary
from .corpus import Corpus, load_corpus, materialize
from .extract import extract_surfaced, is_codecache_query
from .report import score_trajectory
from .trajectory import TrajectoryLogger, TrajectoryMeta

#: How the model is told to emit an action, per interaction mode. Native tool-calling
#: (``litellm``) vs. text bash-blocks (``litellm_textbased`` — robust for small local
#: models like qwen2.5/llama3/phi3 that flake on Ollama native tool calls).
TOOLCALL_PROTOCOL = (
    "Act ONLY by calling the bash tool with exactly one shell command per step; never reply "
    "with prose or an empty message. Read each command's output, then choose the next command."
)
TEXTBASED_PROTOCOL = (
    "Act by writing EXACTLY ONE shell command per step inside a fenced block labelled "
    "mswea_bash_command, with nothing after it, like:\n"
    "```mswea_bash_command\n"
    "grep -rn pattern .\n"
    "```\n"
    "Read the command's output, then issue the next command the same way."
)

SYSTEM_TEMPLATE = (
    "You are a code-search assistant working in a repository. Your job is to locate the "
    "code relevant to the user's query.\n"
    "{action_protocol}\n"
    "{addendum}\n"
    "When you have found the relevant code, finish with a command whose FIRST output line is "
    "exactly COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT followed by your answer "
    "(for example: echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT; echo <where the code lives>)."
)

INSTANCE_TEMPLATE = "Query: {{task}}\n{% if injected_context %}\nRetrieved context:\n{{injected_context}}\n{% endif %}"


def est_tokens(text: str) -> int:
    """Char/4 token estimate (matches the M6/§6.3 heuristic) — for deterministic runs."""
    return max(1, len(text) // 4)


class LoggingAgent(DefaultAgent):
    """DefaultAgent that logs per-turn surfaced files/blocks to a JSONL trajectory."""

    def __init__(self, model, env, *, traj_logger: TrajectoryLogger, repo_files: set[str], repo_dir: Path, **kwargs):
        super().__init__(model, env, **kwargs)
        self.traj = traj_logger
        self.repo_files = repo_files
        self.repo_dir = repo_dir

    def _prompt_estimate(self) -> int:
        return est_tokens("".join(str(m.get("content", "")) for m in self.messages))

    def _log(self, command: str, kind: str, observation: str, model_content: str) -> None:
        files, blocks = extract_surfaced(command, observation, self.repo_files, self.repo_dir)
        self.traj.log_turn(
            action=command,
            action_kind=kind,
            observation=observation[:2000],
            prompt_tokens=self._prompt_estimate(),
            completion_tokens=est_tokens(model_content),
            files_surfaced=files,
            blocks_surfaced=blocks,
        )

    def step(self) -> list[dict]:
        message = self.query()  # appends model msg; raises LimitsExceeded if over limits
        model_content = str(message.get("content", ""))
        actions = message.get("extra", {}).get("actions", [])
        outputs = []
        for action in actions:
            command = action.get("command", "")
            try:
                out = self.env.execute(action)
            except Submitted:
                self._log(command, "submit", "<submitted>", model_content)
                raise
            outputs.append(out)
            kind = "codecache_query" if is_codecache_query(command) else "bash"
            self._log(command, kind, str(out.get("output", "")), model_content)
        return self.add_messages(*self.model.format_observation_messages(message, outputs, self.get_template_vars()))


def _make_output(content: str, command: str) -> dict:
    """A scripted DeterministicModel output running exactly one bash command, no cost."""
    return {"role": "assistant", "content": content, "extra": {"actions": [{"command": command}], "cost": 0.0}}


def _submit_command(answer: str) -> str:
    safe = answer.replace('"', "'")
    return f'echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT && echo "{safe}"'


def scripted_outputs(arm: Arm, task: Task) -> list[dict]:
    """Deterministic, realistic action scripts per arm (offline validation)."""
    answer = f"The query '{task.query}' is handled by code in the retrieved location(s)."
    if arm.name == "A0":
        terms = task.query.split()[0]
        return [
            _make_output(f"Grep the tree for '{terms}'.", f'grep -rn "{terms}" src'),
            _make_output("Read the most relevant file in full.", "cat src/auth/authenticate.py"),
            _make_output("Found it; submit.", _submit_command(answer)),
        ]
    if arm.name == "A1":
        return [
            _make_output("Use the code index.", f'codecache query "{task.query}" --format json'),
            _make_output("Index pinpointed the symbol; submit.", _submit_command(answer)),
        ]
    if arm.name == "A4":
        # Context was injected up front (logged as a synthetic turn); the agent just submits.
        return [_make_output("Relevant code was provided; submit.", _submit_command(answer))]
    raise ValueError(f"no script for arm {arm.name}")


def _format_topk(qr, k: int = 5) -> str:
    lines = []
    for c in qr.raw.get("chunks", [])[:k]:
        first = c.get("chunk_text", "").splitlines()[:1]
        sig = first[0] if first else ""
        lines.append(f"- {c['symbol_name']} ({c['file_path']}): {sig}")
    return "\n".join(lines)


def run_arm(
    arm: Arm,
    task: Task,
    corpus: Corpus,
    runs_dir: Path,
    binary: Path,
    model_factory,
    *,
    step_limit: int | None = None,
    wall_time_limit_seconds: int = 0,
    model_label: str = "deterministic",
    temperature: float = 0.0,
    action_protocol: str = TOOLCALL_PROTOCOL,
) -> Path:
    """Run one arm; returns the trajectory path.

    Two modes, selected by ``step_limit``:

    * ``None`` (default) — *deterministic*: ``model_factory`` is handed the scripted
      :func:`scripted_outputs` and the step budget is exactly the script length.
    * an ``int`` — *live*: the model decides its own actions (``model_factory`` ignores
      its argument); ``step_limit`` bounds the loop and ``wall_time_limit_seconds`` is a
      wall-clock safety net.
    """
    arm_dir = runs_dir / arm.name
    repo = arm_dir / "repo"
    repo.mkdir(parents=True, exist_ok=True)
    written = materialize(corpus, repo)
    repo_files = {p.relative_to(repo).as_posix() for p in written}
    binary_dir = str(Path(binary).parent)
    uses_index = arm.codecache_in_loop or arm.oneshot_inject

    injected = ""
    idx = None
    if uses_index:
        idx = CodeCacheIndex(repo, binary)
        idx.init()
        idx.index()

    meta = TrajectoryMeta(
        arm=arm.name, task_id=task.task_id, model=model_label, temperature=temperature,
        corpus_id=task.corpus_id, query=task.query,
    )
    logger = TrajectoryLogger(arm_dir / "trajectory.jsonl", meta)

    if arm.oneshot_inject:
        qr = idx.query(task.query)
        injected = _format_topk(qr)
        logger.log_turn(
            action="<one-shot top-k injection from index>",
            action_kind="oneshot_inject",
            observation=injected[:2000],
            prompt_tokens=est_tokens(injected),
            completion_tokens=0,
            files_surfaced=qr.files,
            blocks_surfaced=qr.blocks,
        )

    if step_limit is None:  # deterministic: scripted actions, budget = script length
        outputs = scripted_outputs(arm, task)
        model = model_factory(outputs)
        effective_step_limit = len(outputs) + 1
    else:  # live: the model drives; fixed step budget + wall-clock safety net
        model = model_factory(None)
        effective_step_limit = step_limit

    # Expose the codecache binary only to arms that may use it (A0 stays a clean control).
    env = BashEnvironment(cwd=str(repo), extra_path=binary_dir if uses_index else None)
    agent = LoggingAgent(
        model, env,
        traj_logger=logger, repo_files=repo_files, repo_dir=repo,
        system_template=SYSTEM_TEMPLATE.format(addendum=arm.prompt_addendum, action_protocol=action_protocol),
        instance_template=INSTANCE_TEMPLATE,
        step_limit=effective_step_limit,
        cost_limit=0,
        wall_time_limit_seconds=wall_time_limit_seconds,
        output_path=arm_dir / "mini_trajectory.json",
    )
    agent.run(task=task.query, injected_context=injected)
    return arm_dir / "trajectory.jsonl"


def run_all(
    task: Task,
    arms: list[Arm],
    runs_dir: Path,
    model_factory,
    binary: Path | None = None,
    *,
    step_limit: int | None = None,
    wall_time_limit_seconds: int = 0,
    model_label: str = "deterministic",
    temperature: float = 0.0,
    action_protocol: str = TOOLCALL_PROTOCOL,
    continue_on_error: bool = False,
) -> dict:
    """Run every arm on ``task`` and return the assembled metrics report.

    The live keyword arguments (``step_limit`` / ``wall_time_limit_seconds`` /
    ``model_label`` / ``temperature``) pass straight through to :func:`run_arm`; their
    defaults preserve the deterministic behaviour. With ``continue_on_error`` a single
    arm raising (e.g. a flaky live model) records an ``error`` for that arm instead of
    aborting the whole sweep.
    """
    binary = binary or find_codecache_binary()
    corpus = load_corpus(task.corpus_id)
    report: dict = {"task": asdict(_task_summary(task)), "binary": str(binary), "arms": {}}
    for arm in arms:
        try:
            traj = run_arm(
                arm, task, corpus, runs_dir, binary, model_factory,
                step_limit=step_limit, wall_time_limit_seconds=wall_time_limit_seconds,
                model_label=model_label, temperature=temperature, action_protocol=action_protocol,
            )
            report["arms"][arm.name] = {"description": arm.description, **score_trajectory(traj, task)}
        except Exception as e:  # noqa: BLE001 — surfaced into the report for live robustness
            if not continue_on_error:
                raise
            report["arms"][arm.name] = {"description": arm.description, "error": f"{type(e).__name__}: {e}"}
    return report


def _task_summary(task: Task):
    from dataclasses import make_dataclass

    TS = make_dataclass("TS", ["task_id", "corpus_id", "query", "query_type", "n_gold_files", "n_gold_blocks"])
    return TS(task.task_id, task.corpus_id, task.query, task.query_type, len(task.gold_files), len(task.gold_blocks))

# Evals: how well models write Oxie

Models know little about Oxie, and what they know may be from an older version. Oxie helps them
with `oxie guide` (the guide to the installed version, built into the binary), the `AGENTS.md`
that `oxie new` writes, and the Claude Code skill in [`skills/oxie`](../skills/oxie/SKILL.md).
`run.py` measures how much each of these helps: it gives programming tasks to Claude Code and
checks the programs it writes.

```bash
python3 evals/run.py                          # every task, with the skill and without help, on Sonnet
python3 evals/run.py --model opus --reps 3    # another model, three runs of each task
python3 evals/run.py --conditions skill,agents-md,cli,bare
python3 evals/run.py --mode oneshot           # one answer without tools: what the guide alone teaches
python3 evals/run.py --tasks gradebook,fix-the-build
python3 evals/run.py --check                  # no model calls: every reference solution passes its task
```

It needs Python 3.11 or newer, Rust, a shell (macOS or Linux), and Claude Code logged in
(`claude`). The runs are real model calls, billed to your Claude account or API key; the report
shows what they cost. Results go to `evals/results/<date>-<model>-<mode>/` (ignored by git):
`report.md`, `results.jsonl`, and for each run its transcript, the sources it ended with, and its
grade. `--report <dir>` rewrites a report from `results.jsonl`.

## What a run does

1. `run.py` builds `oxie` from this checkout and copies the crates that projects build with to a
   temporary directory, so the runs use this checkout's compiler without pointing at the
   repository and its documentation.
2. For each task and condition, it makes a project with `oxie new`, copies the task's starter
   files into it, and gives the task to `claude -p` there. The agent can use Bash (`oxie`,
   cargo builds, and read-only commands), Read, Write, Edit, Glob, Grep and Skill. Your own
   settings, CLAUDE.md files, memory, plugins and MCP servers aren't loaded, and the system
   prompt asks the agent to work from what it knows, its skills and the `oxie` command, without
   looking for Oxie's documentation or sources elsewhere.
3. It copies the `.oxi` files the agent left in `src/` into a fresh project (so changes to
   `src/main.rs`, `build.rs` or `Cargo.toml` don't count), builds and runs it, and grades it.

| Condition | What the agent gets besides the task |
|---|---|
| `skill` | the skill in `skills/oxie` (or `--skill <dir>`), which sends it to `oxie guide` |
| `agents-md` | the `AGENTS.md` that `oxie new` writes, which sends it to `oxie guide` |
| `cli` | nothing, but `oxie --help` lists `oxie guide` |
| `bare` | nothing, and `oxie guide` is hidden: what the model knows by itself |

In `--mode oneshot` the model answers once, without tools: `skill` puts what `oxie guide`
prints into the system prompt, and `bare` gives only the task.

A run **passes** when the program builds and prints exactly the task's `expected.out`; uses what
the task asks for (`requires` in `task.toml`) and nothing it forbids (`forbids`, and `rust { }`
blocks in every task); has the files the task names; passes `oxie test` with at least `tests`
tests; and doesn't print the expected output literally.

The report also shows, for each condition: how many first builds succeeded, builds, turns,
cost and time per run, the Rust-isms per solution (Rust syntax where Oxie has its own form:
`fn`, `let`, `println!`, `match`, `impl`, `&self`, `Option<`, `;` at line ends, ...), how many
runs read `oxie guide` and loaded the skill, and the runs that looked outside their project
anyway, which are listed with what they read.

## Tasks

Each directory in `tasks/` is one task:

| File | |
|---|---|
| `prompt.md` | the request, written as a user would, ending with the exact output |
| `expected.out` | what the program must print |
| `task.toml` | `features` it exercises, `requires` / `forbids` (regexes over the code without comments; `forbids` also without strings), `files` it must have, and the number of `tests` `oxie test` must pass |
| `solution/src/` | a reference solution, never shown to the model |
| `starter/` | files copied over the new project before the run, for tasks that change existing code |

`cargo test` runs every reference solution (`eval_solutions_pass` in
`crates/oxie-cli/tests/e2e.rs`), and `run.py --check` also grades them, so a task whose solution
stops building or printing its `expected.out` is noticed. To add a task, write and run the
solution first, save what it prints as `expected.out`, and then write the prompt. Keep the output
deterministic: a `HashMap` iterates in a different order every run, so sort before printing.

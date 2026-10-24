#!/usr/bin/env python3
"""Measures how well a model writes Oxie, with and without help.

Every task in evals/tasks asks for a program. A run gives one task to Claude Code (`claude -p`)
in a fresh Oxie project, then builds the .oxi files it wrote in a clean project of its own and
compares the output with the task's expected.out. See evals/README.md.
"""

import argparse
import concurrent.futures
import datetime
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TASKS = ROOT / "evals" / "tasks"
RESULTS = ROOT / "evals" / "results"

# What the agent gets besides the task, per condition.
CONDITIONS = {
    "skill": "the Oxie skill is installed; it sends the agent to `oxie guide`",
    "agents-md": "the project has the AGENTS.md that `oxie new` writes",
    "cli": "no help, but `oxie --help` lists `oxie guide`",
    "bare": "no help, and `oxie guide` is hidden",
}
ONESHOT_CONDITIONS = {
    "skill": "the system prompt holds what `oxie guide` prints",
    "bare": "only the task",
}

# Appended to Claude Code's system prompt in agent runs.
AGENT_RULES = """\
The current directory is an Oxie project made with `oxie new`: Cargo.toml, build.rs and \
src/main.rs are set up, and the program goes in src/main.oxi (other .oxi files in src/ are \
modules). Oxie is a programming language that compiles to Rust; the `oxie` command builds and \
runs it.

This is an evaluation of writing Oxie. Stay in the current directory, and don't look for Oxie \
documentation, examples or source code anywhere else on this computer or online: work from \
what you know, the skills you have, and what the `oxie` command tells you. Write the program \
in Oxie, not Rust: no `rust { }` blocks, and leave src/main.rs, build.rs and Cargo.toml as they \
are. The program must compute its output rather than print it literally. You are done when \
`oxie run` prints exactly what the task asks for.

Change files with the Write and Edit tools. Bash may run `oxie`, cargo builds, and commands that \
only read; anything else is denied."""

# The system prompt of one-shot runs, which have no tools.
ONESHOT_RULES = """\
You write programs in Oxie, a programming language that compiles to Rust. Answer with the \
complete contents of every file of the program: src/main.oxi, and any other .oxi modules in \
src/. Put each file in its own ```oxie code block whose first line is a comment with the \
file's path, like `// src/main.oxi`. Write Oxie, not Rust (no `rust { }` blocks), and compute \
the output rather than printing it literally."""

# `oxie` without `oxie guide`, for the "bare" runs.
BARE_OXIE = """#!/bin/sh
case "$1" in
  guide) echo "error: unknown command \\`guide\\`" >&2; exit 2 ;;
  ""|help|--help|-h) "{oxie}" "$@" | grep -v '^    guide '; exit 0 ;;
esac
exec "{oxie}" "$@"
"""

# Checked in every solution, in code without comments and strings.
FORBIDDEN = [r"\brust\s*\{"]

# Rust written where Oxie has its own form; counted, not failed.
RUST_ISMS = {
    "fn": r"\bfn\s+\w",
    "let": r"\blet\s",
    "macro!": r"\b(?:println|print|eprintln|eprint|format|vec|panic|assert|assert_eq|assert_ne|write|writeln|unreachable|todo|matches)!",
    "match": r"\bmatch\b",
    "impl": r"\bimpl\b",
    "struct": r"\bstruct\s+[A-Z]",
    "&self": r"&\s*(?:mut\s+)?self\b",
    "Option<": r"\bOption\s*<",
    "Result<": r"\bResult\s*<",
    "::new(": r"::new\s*\(",
    "Rc/Arc": r"\b(?:Rc|Arc)\s*<\s*(?:RefCell|Mutex|RwLock)",
    "#[attr]": r"#!?\[",
    "|x| closure": r"[(,=]\s*(?:move\s+)?\|[^|\n]*\|",
    "String::from": r"\bString::from\s*\(",
    "semicolon": r";[ \t]*$",
}

BUILD_COMMAND = re.compile(r"\b(?:oxie\s+(?:run|build|check|test)|cargo\s+(?:run|build|check|test))\b")


class Task:
    def __init__(self, path: Path):
        self.name = path.name
        self.prompt = (path / "prompt.md").read_text()
        self.expected = (path / "expected.out").read_text()
        spec = tomllib.loads((path / "task.toml").read_text()) if (path / "task.toml").exists() else {}
        self.features = spec.get("features", [])
        self.requires = spec.get("requires", [])
        self.forbids = spec.get("forbids", [])
        self.files = spec.get("files", [])
        self.tests = spec.get("tests", 0)
        self.starter = path / "starter"
        self.solution = path / "solution"


def load_tasks(names):
    tasks = [Task(p) for p in sorted(TASKS.iterdir()) if (p / "prompt.md").exists()]
    if names:
        unknown = set(names) - {t.name for t in tasks}
        if unknown:
            sys.exit(f"unknown tasks: {', '.join(sorted(unknown))}")
        tasks = [t for t in tasks if t.name in names]
    return tasks


class Toolchain:
    """`oxie` built from this checkout, and a copy of the crates its projects build with, so the
    runs use this checkout's compiler without pointing at the repository and its docs."""

    def __init__(self, work: Path):
        subprocess.run(["cargo", "build", "-q", "-p", "oxie"], cwd=ROOT, check=True)
        self.home = work / "toolchain"
        for crate in ("oxie-build", "oxie-compiler"):
            shutil.copytree(ROOT / "crates" / crate, self.home / "crates" / crate, ignore=shutil.ignore_patterns("target"))
        package = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]
        manifest = ["[workspace]", 'resolver = "2"', 'members = ["crates/oxie-build", "crates/oxie-compiler"]', "", "[workspace.package]"]
        manifest += [f"{key} = {json.dumps(value)}" for key, value in package.items()]
        manifest += ["", "[workspace.dependencies]", 'oxie-compiler = { path = "crates/oxie-compiler" }', ""]
        (self.home / "Cargo.toml").write_text("\n".join(manifest))
        self.bin = work / "bin"
        self.bin.mkdir()
        self.oxie = self.bin / "oxie"
        shutil.copy2(ROOT / "target" / "debug" / "oxie", self.oxie)
        self.bare_bin = work / "bin-bare"
        self.bare_bin.mkdir()
        (self.bare_bin / "oxie").write_text(BARE_OXIE.replace("{oxie}", str(self.oxie)))
        (self.bare_bin / "oxie").chmod(0o755)
        self.version = subprocess.run([self.oxie, "version"], capture_output=True, text=True).stdout.strip()
        self.guide = subprocess.run([self.oxie, "guide"], capture_output=True, text=True, cwd=work).stdout

    def env(self, run_dir: Path, hide_guide=False):
        env = {k: v for k, v in os.environ.items() if not k.startswith("OXIE_") and k != "CARGO_TARGET_DIR"}
        env["PATH"] = f"{self.bare_bin if hide_guide else self.bin}{os.pathsep}{env.get('PATH', '')}"
        env["OXIE_HOME"] = str(self.home)
        env["OXIE_CACHE"] = str(run_dir / "cache")
        env["NO_COLOR"] = "1"
        env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] = "1"
        return env

    def project(self, parent: Path, starter: Path | None = None) -> Path:
        """A project made by `oxie new`, with the task's starter files copied over it."""
        parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([self.oxie, "new", "app"], cwd=parent, env=self.env(parent), check=True, capture_output=True)
        project = parent / "app"
        if starter and starter.exists():
            shutil.copytree(starter, project, dirs_exist_ok=True)
        return project

    def run(self, args, cwd: Path, timeout: int):
        try:
            return subprocess.run([self.oxie, *args], cwd=cwd, env=self.env(cwd.parent), capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            return subprocess.CompletedProcess(args, -1, "", f"timed out after {timeout}s")


def strip_code(code: str, strings: bool) -> str:
    """`code` with comments (and, when `strings` is set, the text of string and char literals)
    blanked out, keeping its lines."""
    out, i, n = [], 0, len(code)
    blank = lambda s: re.sub(r"[^\n]", " ", s)
    while i < n:
        c = code[i]
        if code.startswith("//", i):
            j = code.find("\n", i)
            j = n if j < 0 else j
            out.append(blank(code[i:j]))
            i = j
        elif code.startswith("/*", i):
            depth, j = 0, i
            while j < n:
                if code.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif code.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                    if depth == 0:
                        break
                else:
                    j += 1
            out.append(blank(code[i:j]))
            i = j
        elif code.startswith('"""', i):
            j = code.find('"""', i + 3)
            j = n if j < 0 else j + 3
            out.append(blank(code[i:j]) if strings else code[i:j])
            i = j
        elif c == '"' or (c == "r" and re.match(r'r#*"', code[i:]) and (i == 0 or not code[i - 1].isalnum())):
            if c == "r":
                hashes = re.match(r"r(#*)\"", code[i:]).group(1)
                end = code.find('"' + hashes, i + 2 + len(hashes))
                j = n if end < 0 else end + 1 + len(hashes)
            else:
                j = string_end(code, i)
            out.append(blank(code[i:j]) if strings else code[i:j])
            i = j
        elif c == "'" and re.match(r"'(?:\\.|\\u\{[0-9a-fA-F]+\}|[^\\'\n])'", code[i:]):
            j = i + re.match(r"'(?:\\.|\\u\{[0-9a-fA-F]+\}|[^\\'\n])'", code[i:]).end()
            out.append(blank(code[i:j]) if strings else code[i:j])
            i = j
        else:
            out.append(c)
            i += 1
    return "".join(out)


def string_end(code: str, i: int) -> int:
    """The index after the `"..."` literal at `i`, whose `${...}` templates may hold strings."""
    j, n = i + 1, len(code)
    while j < n:
        if code[j] == "\\":
            j += 2
        elif code[j] == '"':
            return j + 1
        elif code.startswith("${", j):
            depth, j = 1, j + 2
            while j < n and depth:
                if code[j] == '"':
                    j = string_end(code, j)
                    continue
                depth += {"{": 1, "}": -1}.get(code[j], 0)
                j += 1
        else:
            j += 1
    return n


def return_arrows(code: str) -> int:
    """Function declarations with Rust's `-> T` return type instead of `: T`."""
    count = 0
    for m in re.finditer(r"\bfun\b[^(\n]*\(", code):
        depth, j = 1, m.end()
        while j < len(code) and depth:
            depth += {"(": 1, ")": -1}.get(code[j], 0)
            j += 1
        if re.match(r"\s*->", code[j:]):
            count += 1
    return count


def rust_isms(code: str) -> dict:
    found = {name: len(re.findall(pattern, code, re.M)) for name, pattern in RUST_ISMS.items()}
    found["-> return type"] = return_arrows(code)
    return {name: n for name, n in found.items() if n}


def normalized(text: str) -> str:
    return "\n".join(line.rstrip() for line in text.strip("\n").splitlines())


def first_difference(expected: str, actual: str) -> str:
    want, got = normalized(expected).splitlines(), normalized(actual).splitlines()
    for i in range(max(len(want), len(got))):
        w = want[i] if i < len(want) else "(nothing)"
        g = got[i] if i < len(got) else "(nothing)"
        if w != g:
            return f"line {i + 1}: expected `{w}`, got `{g}`"
    return ""


def grade(task: Task, src: Path, work: Path, toolchain: Toolchain) -> dict:
    """Builds the .oxi files in `src` in a clean project, runs them, and checks them against the
    task."""
    files = sorted(p.relative_to(src).as_posix() for p in src.rglob("*.oxi")) if src.exists() else []
    g = {"pass": False, "files": files, "problems": [], "built": False, "rust_isms": {}}
    if "main.oxi" not in files:
        g["problems"].append("no src/main.oxi")
        return g
    code = "\n".join((src / f).read_text() for f in files)
    without_comments, bare = strip_code(code, strings=False), strip_code(code, strings=True)
    g["rust_isms"] = rust_isms(bare)
    g["problems"] += [f"doesn't match `{p}`" for p in task.requires if not re.search(p, without_comments)]
    g["problems"] += [f"matches forbidden `{p}`" for p in task.forbids + FORBIDDEN if re.search(p, bare, re.M)]
    g["problems"] += [f"has no {f}" for f in task.files if f.removeprefix("src/") not in files]
    lines = [line.strip() for line in task.expected.splitlines() if len(line.strip()) >= 8]
    if len(lines) >= 2 and 2 * sum(line in code for line in lines) > len(lines):
        g["problems"].append("prints the expected output literally")

    project = toolchain.project(work)
    (project / "AGENTS.md").unlink(missing_ok=True)
    for f in files:
        (project / "src" / f).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src / f, project / "src" / f)
    build = toolchain.run(["build"], project, 600)
    g["built"] = build.returncode == 0
    if not g["built"]:
        errors = [line for line in build.stderr.splitlines() if re.match(r"error(\[|:)", line)]
        g["problems"].append("doesn't build: " + (errors[0] if errors else build.stderr.strip()[-300:]))
        g["build_errors"] = errors[:20]
        return g
    run = toolchain.run(["run"], project, 120)
    g["output"] = run.stdout
    if run.returncode != 0:
        g["problems"].append(f"exits with status {run.returncode}: {run.stderr.strip()[-300:]}")
    elif normalized(run.stdout) != normalized(task.expected):
        g["problems"].append("output differs at " + first_difference(task.expected, run.stdout))
    if task.tests:
        test = toolchain.run(["test"], project, 600)
        passed = sum(int(n) for n in re.findall(r"(\d+) passed", test.stdout + test.stderr))
        g["tests_passed"] = passed
        if test.returncode != 0:
            g["problems"].append("`oxie test` fails")
        elif passed < task.tests:
            g["problems"].append(f"{passed} tests pass, the task asks for {task.tests}")
    g["pass"] = not g["problems"]
    return g


def parse_transcript(text: str):
    """The tool calls (with their results) and the final message of a stream-json transcript."""
    calls, by_id, final = [], {}, {}
    for line in text.splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get("type") == "assistant":
            for c in m.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    call = {"tool": c.get("name"), "input": c.get("input") or {}, "output": "", "error": False}
                    by_id[c.get("id")] = call
                    calls.append(call)
        elif m.get("type") == "user" and isinstance(m.get("message", {}).get("content"), list):
            for c in m["message"]["content"]:
                if c.get("type") == "tool_result" and c.get("tool_use_id") in by_id:
                    out = c.get("content")
                    if isinstance(out, list):
                        out = "\n".join(x.get("text", "") for x in out if isinstance(x, dict))
                    by_id[c["tool_use_id"]].update(output=str(out or ""), error=bool(c.get("is_error")))
        elif m.get("type") == "result":
            final = m
    return calls, final


def looked_outside(calls, allowed) -> list:
    """Tool calls that read files in the home directory outside the run (the repository, crate
    sources, caches) or reach the Oxie website, which the rules ask the agent not to."""
    home = Path.home().resolve()
    allowed = [Path(a).resolve() for a in allowed]

    def outside(path: str) -> bool:
        p = Path(os.path.expanduser(path)).resolve()
        return p.is_relative_to(home) and not any(p.is_relative_to(a) for a in allowed)

    found = []
    for c in calls:
        inp = c["input"]
        if c["tool"] == "Bash":
            command = inp.get("command", "")
            paths = re.findall(r"(?<![\w.$])((?:~|/)[^\s'\"|;&)<>]*)", command)
            if "oxie.bkymz.net" in command or "oxie-lang" in command or "$HOME" in command or any(outside(p) for p in paths):
                found.append(command[:200])
        else:
            for key in ("file_path", "path", "notebook_path"):
                if isinstance(inp.get(key), str) and inp[key].startswith(("/", "~")) and outside(inp[key]):
                    found.append(f"{c['tool']} {inp[key]}")
    return found


def agent_metrics(calls, final, allowed) -> dict:
    ran = [c for c in calls if c["tool"] == "Bash" and not (c["error"] and c["output"].startswith("Permission for this tool use was denied"))]
    commands = [c["input"].get("command", "") for c in ran]
    builds = [c for c in ran if BUILD_COMMAND.search(c["input"].get("command", ""))]
    first_build_ok = None
    if builds:
        first = builds[0]
        first_build_ok = not first["error"] and not re.search(r"^(?:error(?:\[|:)|Exit code [1-9])", first["output"], re.M)
    usage = final.get("usage") or {}
    return {
        "turns": final.get("num_turns"),
        "cost_usd": final.get("total_cost_usd"),
        "tokens": sum(usage.get(k) or 0 for k in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens")),
        "stop": final.get("subtype") or "no result",
        "tool_calls": len(calls),
        "builds": len(builds),
        "first_build_ok": first_build_ok,
        "guide": [c for c in commands if re.search(r"\boxie\s+guide\b", c)],
        "skill_used": any(c["tool"] == "Skill" and "oxie" in json.dumps(c["input"]) for c in calls),
        "denied": len(final.get("permission_denials") or []),
        "outside": looked_outside(calls, allowed),
    }


def run_process(cmd, cwd: Path, env: dict, timeout: int):
    """Runs `cmd` in a process group of its own, so a timeout also stops what it started."""
    proc = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        out, err = proc.communicate(timeout=timeout)
        return out, err, False
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        return out, err, True


class Eval:
    def __init__(self, args, tasks, out: Path, work: Path):
        self.args, self.tasks, self.out, self.work = args, tasks, out, work
        self.toolchain = Toolchain(work)
        self.plugin = work / "plugin"
        (self.plugin / ".claude-plugin").mkdir(parents=True)
        (self.plugin / ".claude-plugin" / "plugin.json").write_text(json.dumps({"name": "oxie", "description": "The Oxie skill under evaluation"}))
        shutil.copytree(Path(args.skill).resolve(), self.plugin / "skills" / "oxie")

    def agent_command(self, task: Task, condition: str):
        a = self.args
        cmd = ["claude", "-p", "--output-format", "stream-json", "--verbose", "--model", a.model, "--max-turns", str(a.max_turns)]
        cmd += ["--setting-sources", "project", "--strict-mcp-config", "--no-session-persistence"]
        cmd += ["--tools", "Bash,Read,Write,Edit,Glob,Grep,Skill"]
        cmd += ["--allowedTools", "Bash(oxie:*)", "Bash(cargo build:*)", "Bash(cargo run:*)", "Bash(cargo check:*)", "Bash(cargo test:*)", "Read", "Write", "Edit", "Glob", "Grep", "Skill"]
        cmd += ["--permission-prompts", "none", "--append-system-prompt", AGENT_RULES]
        if condition == "skill":
            cmd += ["--plugin-dir", str(self.plugin)]
        if a.effort:
            cmd += ["--effort", a.effort]
        return cmd + ["--", task.prompt]

    def oneshot_command(self, task: Task, condition: str):
        system = ONESHOT_RULES + ("\n\n" + self.toolchain.guide if condition == "skill" else "")
        prompt = task.prompt
        if task.starter.exists():
            for f in sorted(task.starter.rglob("*.oxi")):
                prompt += f"\n\n{f.relative_to(task.starter).as_posix()} currently contains:\n\n```oxie\n{f.read_text()}```"
        cmd = ["claude", "-p", "--output-format", "json", "--model", self.args.model, "--max-turns", "1", "--tools", ""]
        cmd += ["--setting-sources", "project", "--strict-mcp-config", "--no-session-persistence", "--system-prompt", system]
        if self.args.effort:
            cmd += ["--effort", self.args.effort]
        return cmd + ["--", prompt]

    def run_one(self, task: Task, condition: str, rep: int) -> dict:
        run_dir = self.work / "runs" / f"{task.name}-{condition}-{rep}"
        saved = self.out / "runs" / task.name / f"{condition}-{rep}"
        saved.mkdir(parents=True, exist_ok=True)
        record = {"task": task.name, "condition": condition, "rep": rep, "mode": self.args.mode}
        start = time.time()
        if self.args.mode == "agent":
            project = self.toolchain.project(run_dir, task.starter)
            if condition != "agents-md":
                (project / "AGENTS.md").unlink()
            env = self.toolchain.env(run_dir, hide_guide=condition == "bare")
            out, err, timed_out = run_process(self.agent_command(task, condition), project, env, self.args.timeout)
            (saved / "transcript.jsonl").write_text(out)
            calls, final = parse_transcript(out)
            record.update(agent_metrics(calls, final, [run_dir, self.plugin]))
            src = project / "src"
        else:
            run_dir.mkdir(parents=True)
            out, err, timed_out = run_process(self.oneshot_command(task, condition), run_dir, self.toolchain.env(run_dir), self.args.timeout)
            try:
                final = json.loads(out)
            except ValueError:
                final = {}
            answer = final.get("result") or ""
            (saved / "answer.md").write_text(answer)
            src = run_dir / "src"
            for name, text in extract_files(answer).items():
                (src / name).parent.mkdir(parents=True, exist_ok=True)
                (src / name).write_text(text)
            usage = final.get("usage") or {}
            record.update(cost_usd=final.get("total_cost_usd"), turns=final.get("num_turns"), stop=final.get("subtype") or "no result",
                          tokens=sum(usage.get(k) or 0 for k in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens")))
        record["seconds"] = round(time.time() - start, 1)
        record["timed_out"] = timed_out
        if err.strip() and not out.strip():
            record["claude_error"] = err.strip()[-500:]
        record.update(grade(task, src, run_dir / "grade", self.toolchain))
        if src.exists():
            shutil.copytree(src, saved / "src", ignore=shutil.ignore_patterns("*.rs"), dirs_exist_ok=True)
        (saved / "grade.json").write_text(json.dumps(record, indent=2) + "\n")
        if not self.args.keep:
            shutil.rmtree(run_dir, ignore_errors=True)
        return record


def extract_files(answer: str) -> dict:
    """The files of a one-shot answer: each code block, named by its `// src/...` first line."""
    files = {}
    for m in re.finditer(r"```[\w-]*\n(.*?)```", answer, re.S):
        body = m.group(1)
        first = body.split("\n", 1)[0].strip()
        named = re.match(r"//\s*(?:src/)?([\w/.-]+\.oxi)\s*$", first)
        if named:
            files[named.group(1)] = body
        elif "main.oxi" not in files and "fun main" in body:
            files["main.oxi"] = body
    return files


def mean(values):
    values = [v for v in values if v is not None]
    return sum(values) / len(values) if values else None


def duration(seconds):
    return "–" if seconds is None else f"{int(seconds // 60)}m {int(seconds % 60):02d}s"


def report(records, config) -> str:
    conditions = config["conditions"]
    by = {c: [r for r in records if r["condition"] == c] for c in conditions}
    agent = config["mode"] == "agent"

    def row(label, value):
        return f"| {label} | " + " | ".join(value(by[c]) for c in conditions) + " |"

    def count(rs, key, want=True):
        known = [r for r in rs if r.get(key) is not None]
        return f"{sum(r.get(key) == want for r in known)}/{len(known)}" if known else "–"

    def avg(rs, key, fmt):
        m = mean([r.get(key) for r in rs])
        return "–" if m is None else fmt(m)

    lines = [f"# Oxie eval: {config['model']}, {config['mode']} mode", ""]
    lines.append(f"{config['date']}. {config['oxie']} (commit {config['commit']}), Claude Code {config['claude']}. "
                 f"{len(config['tasks'])} tasks, {config['reps']} run(s) per task and condition.")
    lines += ["", *(f"* **{c}**: {(CONDITIONS if agent else ONESHOT_CONDITIONS)[c]}" for c in conditions), ""]
    lines += ["| | " + " | ".join(conditions) + " |", "|---|" + "---|" * len(conditions)]
    lines.append(row("tasks passed", lambda rs: f"{sum(r['pass'] for r in rs)}/{len(rs)} ({100 * sum(r['pass'] for r in rs) // max(len(rs), 1)}%)"))
    lines.append(row("solutions that build", lambda rs: count(rs, "built")))
    if agent:
        lines.append(row("first build succeeded", lambda rs: count(rs, "first_build_ok")))
        lines.append(row("builds per run", lambda rs: avg(rs, "builds", lambda m: f"{m:.1f}")))
        lines.append(row("turns per run", lambda rs: avg(rs, "turns", lambda m: f"{m:.1f}")))
    lines.append(row("cost per run", lambda rs: avg(rs, "cost_usd", lambda m: f"${m:.2f}")))
    lines.append(row("time per run", lambda rs: duration(mean([r.get("seconds") for r in rs]))))
    lines.append(row("Rust-isms per solution", lambda rs: avg([dict(r, n=sum(r.get("rust_isms", {}).values())) for r in rs], "n", lambda m: f"{m:.1f}")))
    if agent:
        lines.append(row("ran `oxie guide`", lambda rs: f"{sum(bool(r.get('guide')) for r in rs)}/{len(rs)}"))
        lines.append(row("loaded the skill", lambda rs: f"{sum(bool(r.get('skill_used')) for r in rs)}/{len(rs)}"))
        lines.append(row("looked outside the project", lambda rs: f"{sum(bool(r.get('outside')) for r in rs)}/{len(rs)}"))
        lines.append(row("commands denied per run", lambda rs: avg(rs, "denied", lambda m: f"{m:.1f}")))
        lines.append(row("hit the turn limit or timed out", lambda rs: f"{sum(r.get('timed_out') or r.get('stop') == 'error_max_turns' for r in rs)}/{len(rs)}"))
    total = sum(r.get("cost_usd") or 0 for r in records)
    lines += ["", f"Total cost: ${total:.2f}.", "", "## Tasks", ""]
    lines += ["| task | exercises | " + " | ".join(conditions) + " |", "|---|---|" + "---|" * len(conditions)]
    for task in config["tasks"]:
        cells = []
        for c in conditions:
            rs = [r for r in by[c] if r["task"] == task]
            marks = "".join("✓" if r["pass"] else "✗" for r in rs)
            extra = f" {mean([r.get('turns') for r in rs]):.0f} turns" if agent and mean([r.get("turns") for r in rs]) is not None else ""
            cells.append(f"{marks}{extra}")
        lines.append(f"| {task} | {', '.join(config.get('features', {}).get(task, []))} | " + " | ".join(cells) + " |")
    lines += ["", "## Problems", ""]
    for r in sorted(records, key=lambda r: (r["task"], conditions.index(r["condition"]), r["rep"])):
        notes = list(r.get("problems", []))
        if r.get("claude_error"):
            notes.append("claude: " + r["claude_error"].splitlines()[-1])
        if r.get("outside"):
            notes.append("looked outside: " + "; ".join(r["outside"][:3]))
        if r.get("rust_isms"):
            notes.append("Rust-isms: " + ", ".join(f"{k} ×{v}" for k, v in r["rust_isms"].items()))
        if notes:
            lines.append(f"* **{r['task']}**, {r['condition']} #{r['rep']}: " + "; ".join(notes))
    return "\n".join(lines) + "\n"


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True).stdout.strip()


def check(tasks) -> int:
    """Grades every task's reference solution: it must pass its own task."""
    work = Path(tempfile.mkdtemp(prefix="oxie-eval-check-"))
    try:
        toolchain = Toolchain(work)

        def one(task):
            return task, grade(task, task.solution / "src", work / task.name, toolchain)

        failed = 0
        with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
            for task, g in pool.map(one, tasks):
                status = "ok" if g["pass"] else "FAILED: " + "; ".join(g["problems"])
                isms = f" (Rust-isms: {g['rust_isms']})" if g["rust_isms"] else ""
                print(f"{task.name:<22} {status}{isms}")
                failed += not g["pass"]
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description="Measure how well a model writes Oxie, with and without help.")
    parser.add_argument("--model", default="sonnet", help="model for `claude --model` (default: sonnet)")
    parser.add_argument("--mode", choices=["agent", "oneshot"], default="agent", help="agent: Claude Code with tools; oneshot: one answer, no tools")
    parser.add_argument("--conditions", default="skill,bare", help=f"comma-separated, from {', '.join(CONDITIONS)} (default: skill,bare)")
    parser.add_argument("--tasks", default="", help="comma-separated task names (default: all)")
    parser.add_argument("--reps", type=int, default=1, help="runs per task and condition (default: 1)")
    parser.add_argument("--jobs", type=int, default=4, help="runs at the same time (default: 4)")
    parser.add_argument("--max-turns", type=int, default=60, help="turn limit of an agent run (default: 60)")
    parser.add_argument("--timeout", type=int, default=1800, help="seconds before a run is stopped (default: 1800)")
    parser.add_argument("--effort", default="", help="`claude --effort` level")
    parser.add_argument("--skill", default=str(ROOT / "skills" / "oxie"), help="the skill directory to evaluate (default: skills/oxie)")
    parser.add_argument("--name", default="", help="name of the results directory")
    parser.add_argument("--keep", action="store_true", help="keep the projects the runs worked in")
    parser.add_argument("--check", action="store_true", help="only check that every task's reference solution passes")
    parser.add_argument("--report", metavar="DIR", help="rewrite DIR/report.md from DIR/results.jsonl")
    args = parser.parse_args()

    tasks = load_tasks([t for t in args.tasks.split(",") if t])
    if args.check:
        sys.exit(check(tasks))
    if args.report:
        out = Path(args.report)
        config = json.loads((out / "config.json").read_text())
        records = [json.loads(line) for line in (out / "results.jsonl").read_text().splitlines() if line.strip()]
        (out / "report.md").write_text(report(records, config))
        print(out / "report.md")
        return

    conditions = [c for c in args.conditions.split(",") if c]
    allowed = CONDITIONS if args.mode == "agent" else ONESHOT_CONDITIONS
    if not conditions or any(c not in allowed for c in conditions):
        sys.exit(f"--conditions must be from {', '.join(allowed)} in {args.mode} mode")
    if shutil.which("claude") is None:
        sys.exit("needs Claude Code: the `claude` command isn't on the PATH")

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = RESULTS / (args.name or f"{stamp}-{args.model}-{args.mode}")
    out.mkdir(parents=True, exist_ok=False)
    work = Path(tempfile.mkdtemp(prefix="oxie-eval-"))
    ev = Eval(args, tasks, out, work)
    dirty = " + uncommitted changes" if git("status", "--porcelain") else ""
    config = {
        "date": datetime.datetime.now().strftime("%Y-%m-%d %H:%M"),
        "model": args.model, "mode": args.mode, "conditions": conditions, "reps": args.reps,
        "tasks": [t.name for t in tasks], "features": {t.name: t.features for t in tasks},
        "max_turns": args.max_turns, "effort": args.effort,
        "oxie": ev.toolchain.version, "commit": git("rev-parse", "--short", "HEAD") + dirty,
        "claude": subprocess.run(["claude", "--version"], capture_output=True, text=True).stdout.strip(),
        "skill": str(Path(args.skill).resolve().relative_to(ROOT)) if Path(args.skill).resolve().is_relative_to(ROOT) else args.skill,
    }
    (out / "config.json").write_text(json.dumps(config, indent=2) + "\n")
    jobs = [(t, c, rep) for rep in range(1, args.reps + 1) for t in tasks for c in conditions]
    print(f"{len(jobs)} runs of {args.model} ({args.mode}) into {out.relative_to(ROOT)}", flush=True)
    records = []
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = {pool.submit(ev.run_one, *job): job for job in jobs}
            for future in concurrent.futures.as_completed(futures):
                task, condition, rep = futures[future]
                try:
                    r = future.result()
                except Exception as e:  # a broken run is reported, not fatal
                    r = {"task": task.name, "condition": condition, "rep": rep, "pass": False, "problems": [f"the harness failed: {e!r}"]}
                records.append(r)
                with open(out / "results.jsonl", "a") as f:
                    f.write(json.dumps(r) + "\n")
                cost = f", ${r['cost_usd']:.2f}" if r.get("cost_usd") is not None else ""
                turns = f", {r['turns']} turns" if r.get("turns") is not None else ""
                verdict = "pass" if r["pass"] else "FAIL: " + "; ".join(r.get("problems", []))[:160]
                print(f"[{len(records)}/{len(jobs)}] {task.name} {condition}#{rep}: {verdict}{turns}{cost}", flush=True)
    finally:
        if not args.keep:
            shutil.rmtree(work, ignore_errors=True)
    (out / "report.md").write_text(report(records, config))
    print(f"\n{out.relative_to(ROOT) / 'report.md'}")


if __name__ == "__main__":
    main()

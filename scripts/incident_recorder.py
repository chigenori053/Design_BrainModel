#!/usr/bin/env python3
"""Record structured incidents only when command execution fails."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from typing import Sequence

INCIDENT_ROOT = Path("runtime/incidents")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_bytes(path: Path) -> bytes:
    with path.open("rb") as f:
        return f.read()


def compute_execution_id(spec_path: Path, task_path: Path) -> str:
    return sha256_bytes(load_bytes(spec_path) + b"\n" + load_bytes(task_path))


def classify_incident(exit_code: int, stderr: str, timed_out: bool) -> str:
    text = stderr.lower()
    if timed_out:
        return "timeout"
    if "sandbox" in text or "operation not permitted" in text:
        return "sandbox_violation"
    if "permission denied" in text or "read-only file system" in text:
        return "security_violation"
    if "panicked at" in text or "thread 'main' panicked" in text or "panic" in text:
        return "panic"
    if "could not compile" in text or "error[" in text or "cannot find" in text:
        return "compile_error"
    if "test failed" in text or "assertion failed" in text:
        return "test_failure"
    if exit_code != 0:
        return "non_zero_exit"
    return "none"


def summarize(stderr: str) -> str:
    for line in stderr.splitlines():
        trimmed = line.strip()
        if trimmed:
            return trimmed[:160]
    return "command failed without stderr output"


def rust_version() -> str:
    try:
        proc = subprocess.run(
            ["rustc", "--version"],
            check=False,
            capture_output=True,
            text=True,
            timeout=5,
        )
        out = proc.stdout.strip()
        return out if out else "unknown"
    except (subprocess.SubprocessError, OSError):
        return "unknown"


def utc_timestamp() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def ensure_output_path(execution_id: str, now_utc: datetime) -> Path:
    day_path = INCIDENT_ROOT / now_utc.date().isoformat()
    day_path.mkdir(parents=True, exist_ok=True)
    return day_path / f"{execution_id}.json"


def write_incident(
    output_path: Path,
    execution_id: str,
    task_id: str,
    command: Sequence[str],
    exit_code: int,
    category: str,
    stderr: str,
) -> None:
    incident = {
        "execution_id": execution_id,
        "timestamp": utc_timestamp(),
        "task_id": task_id,
        "command": " ".join(command),
        "exit_code": exit_code,
        "category": category,
        "summary": summarize(stderr),
        "deterministic_hash": sha256_bytes(stderr.encode("utf-8", errors="replace")),
        "environment": {
            "os": platform.system(),
            "rust_version": rust_version(),
        },
    }
    with output_path.open("w", encoding="utf-8") as f:
        json.dump(incident, f, ensure_ascii=True, indent=2)
        f.write("\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Execute command and record incident only on abnormal termination.")
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--spec", required=True, help="Path to spec file used for execution_id.")
    parser.add_argument("--task", required=True, help="Path to task yaml used for execution_id.")
    parser.add_argument("--timeout-sec", type=int, default=300)
    parser.add_argument("command", nargs=argparse.REMAINDER, help="Command to execute, prefixed by --")
    args = parser.parse_args()
    if not args.command:
        parser.error("command is required. Usage: ... -- <command> [args]")
    if args.command[0] == "--":
        args.command = args.command[1:]
    return args


def main() -> int:
    args = parse_args()
    spec_path = Path(args.spec)
    task_path = Path(args.task)
    if not spec_path.exists():
        print(f"ERROR: spec file not found: {spec_path}", file=sys.stderr)
        return 2
    if not task_path.exists():
        print(f"ERROR: task file not found: {task_path}", file=sys.stderr)
        return 2

    execution_id = compute_execution_id(spec_path, task_path)
    timed_out = False
    exit_code = 0
    stdout = ""
    stderr = ""

    try:
        proc = subprocess.run(
            args.command,
            check=False,
            capture_output=True,
            text=True,
            timeout=args.timeout_sec,
        )
        exit_code = proc.returncode
        stdout = proc.stdout
        stderr = proc.stderr
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        exit_code = 124
        stdout = (exc.stdout or "")
        stderr = (exc.stderr or "")
        stderr = f"{stderr}\ncommand timed out after {args.timeout_sec} seconds".strip()

    if stdout:
        print(stdout, end="")
    if stderr:
        print(stderr, end="", file=sys.stderr)

    category = classify_incident(exit_code, stderr, timed_out)
    if exit_code == 0 and not timed_out:
        return 0

    now_utc = datetime.now(timezone.utc)
    out_path = ensure_output_path(execution_id, now_utc)
    write_incident(
        output_path=out_path,
        execution_id=execution_id,
        task_id=args.task_id,
        command=args.command,
        exit_code=exit_code,
        category=category,
        stderr=stderr,
    )
    print(f"Incident recorded: {out_path}", file=sys.stderr)
    return exit_code if exit_code != 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())

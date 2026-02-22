#!/usr/bin/env python3
"""Task state manager for DesignBrainModel."""

from __future__ import annotations

import argparse
from datetime import date
import os
from pathlib import Path
import sys

import yaml

STATE_PATH = Path("state/TASK_STATE.yaml")
CHANGELOG_PATH = Path("state/CHANGELOG.yaml")

ALLOWED_TRANSITIONS = {
    "proposed": ["approved"],
    "approved": ["in_progress"],
    "in_progress": ["review", "blocked"],
    "review": ["completed"],
    "blocked": ["in_progress"],
}


def load_state() -> dict:
    if not STATE_PATH.exists():
        raise FileNotFoundError(f"State file not found: {STATE_PATH}")
    with STATE_PATH.open("r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def save_state(state: dict) -> None:
    state.setdefault("meta", {})
    state["meta"]["last_updated"] = date.today().isoformat()
    with STATE_PATH.open("w", encoding="utf-8") as f:
        yaml.safe_dump(state, f, sort_keys=False, allow_unicode=False)


def validate_transition(old: str, new: str) -> bool:
    return new in ALLOWED_TRANSITIONS.get(old, [])


def today_date() -> str:
    return date.today().isoformat()


def check_spec_exists(task: dict) -> bool:
    related_spec = task.get("related_spec")
    if not related_spec:
        return False
    return os.path.exists(related_spec)


def validate_spec_structure(path: str) -> bool:
    required_sections = [
        "## Purpose",
        "## Interfaces",
        "## Data Structures",
    ]
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return all(section in content for section in required_sections)


def get_task(state: dict, task_id: str) -> dict:
    for task in state.get("tasks", []):
        if task.get("id") == task_id:
            return task
    raise ValueError(f"Task not found: {task_id}")


def list_current_phase_tasks(state: dict) -> int:
    phase = state.get("current_phase")
    roadmap = state.get("roadmap", {})
    phase_task_ids = roadmap.get(phase, [])
    tasks = {t.get("id"): t for t in state.get("tasks", [])}

    print(f"Current phase: {phase}")
    if not phase_task_ids:
        print("No tasks assigned to current phase.")
        return 0

    for task_id in phase_task_ids:
        task = tasks.get(task_id)
        if not task:
            print(f"- {task_id}: missing from tasks list")
            continue
        print(
            f"- {task_id} [{task.get('status')}] "
            f"{task.get('title')} (owner={task.get('owner')}, priority={task.get('priority')})"
        )
    return 0


def check_dependencies(state: dict, task_id: str) -> int:
    task = get_task(state, task_id)
    deps = task.get("depends_on", [])
    if not deps:
        print(f"{task_id}: no dependencies")
        return 0

    tasks = {t.get("id"): t for t in state.get("tasks", [])}
    blocking = []
    for dep_id in deps:
        dep = tasks.get(dep_id)
        if not dep:
            blocking.append((dep_id, "missing"))
            continue
        dep_status = dep.get("status")
        if dep_status != "completed":
            blocking.append((dep_id, dep_status))

    if not blocking:
        print(f"{task_id}: all dependencies satisfied")
        return 0

    print(f"{task_id}: dependency check failed")
    for dep_id, dep_status in blocking:
        print(f"- {dep_id}: {dep_status}")
    return 1


def update_status(state: dict, task_id: str, new_status: str) -> int:
    task = get_task(state, task_id)
    old_status = task.get("status")

    if old_status == new_status:
        print(f"{task_id}: already in status '{new_status}'")
        return 0

    if not validate_transition(old_status, new_status):
        print(f"Invalid transition: {old_status} -> {new_status}")
        return 2

    if new_status == "approved":
        task["architect_approved"] = True
        task["approved_at"] = today_date()

    if new_status == "in_progress":
        if not task.get("architect_approved", False):
            print("ERROR: Architect approval required.")
            return 2

        governance = state.get("governance", {})
        if governance.get("spec_required_for_progress", False) and not check_spec_exists(task):
            print("ERROR: Related spec file does not exist.")
            return 2

        if not validate_spec_structure(task["related_spec"]):
            print("ERROR: Spec missing required structure.")
            return 2

        dep_rc = check_dependencies(state, task_id)
        if dep_rc != 0:
            return dep_rc

    if new_status == "completed":
        governance = state.get("governance", {})
        if governance.get("validation_required_before_completion", False):
            if task.get("status") != "review":
                print("ERROR: Task must be in review before completion.")
                return 2

    task["status"] = new_status
    save_state(state)
    if new_status == "completed":
        append_changelog(task)
    print(f"{task_id}: {old_status} -> {new_status}")
    check_phase_completion(state)
    return 0


def append_changelog(task: dict) -> None:
    today = date.today().isoformat()
    task_id = task.get("id", "")
    title = task.get("title", "")
    entry = {
        "date": today,
        "type": "task_completion",
        "description": f"Completed {task_id} {title}",
        "related_tasks": [task_id],
    }

    if CHANGELOG_PATH.exists():
        with CHANGELOG_PATH.open("r", encoding="utf-8") as f:
            data = yaml.safe_load(f) or {}
    else:
        data = {"changes": []}

    data.setdefault("changes", [])
    data["changes"].append(entry)

    with CHANGELOG_PATH.open("w", encoding="utf-8") as f:
        yaml.safe_dump(data, f, sort_keys=False, allow_unicode=False)


def check_phase_completion(state: dict) -> None:
    phase_tasks = state.get("roadmap", {}).get(state.get("current_phase"), [])
    all_completed = all(
        any(t.get("id") == tid and t.get("status") == "completed" for t in state.get("tasks", []))
        for tid in phase_tasks
    )
    if all_completed:
        print("Phase complete. Architect approval required to advance.")


def mark_completed(state: dict, task_id: str) -> int:
    return update_status(state, task_id, "completed")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Manage TASK_STATE.yaml")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="List tasks in current phase")

    deps_p = sub.add_parser("deps", help="Check dependencies for a task")
    deps_p.add_argument("task_id")

    status_p = sub.add_parser("status", help="Update a task status")
    status_p.add_argument("task_id")
    status_p.add_argument("new_status")

    done_p = sub.add_parser("complete", help="Mark task as completed from review status")
    done_p.add_argument("task_id")

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    try:
        state = load_state()
        if args.command == "list":
            return list_current_phase_tasks(state)
        if args.command == "deps":
            return check_dependencies(state, args.task_id)
        if args.command == "status":
            return update_status(state, args.task_id, args.new_status)
        if args.command == "complete":
            return mark_completed(state, args.task_id)
        parser.print_help()
        return 2
    except (FileNotFoundError, ValueError) as e:
        print(str(e), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())

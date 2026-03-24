#!/usr/bin/env python3

import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check_analyze_result.py <result.json>", file=sys.stderr)
        return 2

    payload = json.loads(Path(sys.argv[1]).read_text())
    failures = []

    cycles = payload.get("cycles", {})
    if cycles.get("has_cycle") or cycles.get("cycles"):
        failures.append("cycle detected")

    if payload.get("violations"):
        failures.append("layer violations detected")

    summary = payload.get("summary", {})
    if summary.get("critical", 0) != 0:
        failures.append("critical issues detected")
    if summary.get("high", 0) != 0:
        failures.append("high issues detected")

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("analyze result is clean")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

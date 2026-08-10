#!/usr/bin/env python3
"""Generate or verify provider-owned final delivery JSON Schema fixtures."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

TOOL_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOL_ROOT))

from supply_chain_planner.delivery_schemas import (  # noqa: E402
    DELIVERY_SCHEMA_SOURCES,
    model_schema,
)

SCHEMA_DIR = TOOL_ROOT / "contracts" / "schemas"


def canonical_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def fixture_text(schema_name: str) -> str:
    return canonical_json(model_schema(schema_name))


def check_fixtures() -> list[str]:
    errors: list[str] = []
    for schema_name in sorted(DELIVERY_SCHEMA_SOURCES):
        path = SCHEMA_DIR / f"{schema_name}.schema.json"
        expected = fixture_text(schema_name)
        if not path.is_file():
            errors.append(f"missing {path}")
            continue
        if path.read_text(encoding="utf-8") != expected:
            errors.append(f"drift {path}")
    return errors


def write_fixtures() -> None:
    SCHEMA_DIR.mkdir(parents=True, exist_ok=True)
    for schema_name in sorted(DELIVERY_SCHEMA_SOURCES):
        (SCHEMA_DIR / f"{schema_name}.schema.json").write_text(
            fixture_text(schema_name), encoding="utf-8"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="fail when fixtures drift")
    mode.add_argument("--write", action="store_true", help="write deterministic fixtures")
    args = parser.parse_args()
    if args.write:
        write_fixtures()
        return 0
    errors = check_fixtures()
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

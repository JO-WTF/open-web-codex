"""Static validation for a composed Copilot package manifest."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class CopilotPackageError(ValueError):
    """A typed validation failure for a Copilot package."""

    def __init__(self, code: str, relative_path: str, message: str) -> None:
        self.code = code
        self.relative_path = relative_path
        self.message = message
        super().__init__(f"{code}: {relative_path}: {message}")


@dataclass(frozen=True)
class CopilotPackageSummary:
    """Stable, bounded result returned after manifest validation."""

    id: str
    display_name: str
    skill_ids: tuple[str, ...]
    agent_ids: tuple[str, ...]
    tool_ids: tuple[str, ...]
    test_ids: tuple[str, ...]
    composition_descriptor_sha256: str


@dataclass(frozen=True)
class CopilotTestCase:
    """One bounded native Runtime acceptance case declared by the author."""

    id: str
    prompt: str
    agent: str
    tool: str
    tool_name: str
    arguments: dict[str, Any]
    expected_structured_content: dict[str, Any]


def validate_copilot_package(
    source_root: Path, manifest_path: Path | None = None
) -> CopilotPackageSummary:
    """Validate and summarize a composed Copilot package."""

    root = Path(source_root)
    if not root.exists():
        _fail("source_root_invalid", ".", "source root does not exist")
    if root.is_symlink():
        _fail("unsafe_symlink", ".", "source root must not be a symlink")
    if not root.is_dir():
        _fail("source_root_invalid", ".", "source root must be a directory")
    root = root.resolve(strict=True)

    manifest_relative = _manifest_relative_path(root, manifest_path)
    manifest_file = _safe_package_path(root, manifest_relative, kind="file")
    manifest = _load_toml(manifest_file, manifest_relative.as_posix())

    schema_version = _required(manifest, "schema_version", "copilot.toml")
    if type(schema_version) is not int:
        _fail("invalid_type", "schema_version", "must be an integer")
    if schema_version != 1:
        _fail("schema_version", "schema_version", "must equal 1")

    copilot_id = _required_string(manifest, "id", "copilot.toml")
    display_name = _required_string(manifest, "display_name", "copilot.toml")
    skills = _component_entries(manifest, "skills", "path")
    agents = _component_entries(manifest, "agents", "role")
    tools = _component_entries(manifest, "tools", "root")

    skill_ids = tuple(entry["id"] for entry in skills)
    agent_ids = tuple(entry["id"] for entry in agents)
    tool_ids = tuple(entry["id"] for entry in tools)
    tests = _test_entries(manifest, agent_ids=agent_ids, tool_ids=tool_ids)

    supervisor = _required(manifest, "supervisor", "copilot.toml")
    if not isinstance(supervisor, dict):
        _fail("invalid_type", "supervisor", "must be a table")
    supervisor_skill = _required_string(supervisor, "skill", "supervisor")
    if supervisor_skill not in skill_ids:
        _fail(
            "missing_reference",
            "supervisor.skill",
            f"references undeclared skill {supervisor_skill!r}",
        )

    author_files: dict[str, Path] = {
        manifest_relative.as_posix(): manifest_file,
    }

    for index, entry in enumerate(skills):
        skill_id = entry["id"]
        relative = _authored_relative_path(entry["path"], f"skills[{index}].path")
        skill_root = _safe_package_path(root, relative, kind="directory")
        skill_file_relative = relative / "SKILL.md"
        skill_file = _safe_package_path(root, skill_file_relative, kind="file")
        frontmatter_name = _skill_frontmatter_name(
            skill_file, skill_file_relative.as_posix()
        )
        if frontmatter_name != skill_id:
            _fail(
                "frontmatter_mismatch",
                skill_file_relative.as_posix(),
                f"frontmatter name {frontmatter_name!r} does not match {skill_id!r}",
            )
        # Retain the directory check as part of the explicit package contract.
        del skill_root
        author_files[skill_file_relative.as_posix()] = skill_file

    agent_tool_policies: dict[str, dict[str, set[str] | None]] = {}
    for index, entry in enumerate(agents):
        relative = _authored_relative_path(entry["role"], f"agents[{index}].role")
        role_file = _safe_package_path(root, relative, kind="file")
        role = _load_toml(role_file, relative.as_posix())
        role_name = _required_string(role, "name", relative.as_posix())
        if role_name != entry["id"]:
            _fail(
                "missing_reference",
                f"agents[{index}].id",
                f"agent id {entry['id']!r} does not match role name {role_name!r}",
            )
        _validate_role_references(role, relative.as_posix(), skill_ids, tool_ids)
        agent_tool_policies[entry["id"]] = _role_tool_policies(
            role, relative.as_posix(), tool_ids
        )
        author_files[relative.as_posix()] = role_file

    for index, test in enumerate(tests):
        policy = agent_tool_policies[test.agent].get(test.tool)
        if policy is None and test.tool not in agent_tool_policies[test.agent]:
            _fail(
                "missing_reference",
                f"tests[{index}].tool",
                f"agent {test.agent!r} does not enable declared tool {test.tool!r}",
            )
        if policy is not None and test.tool_name not in policy:
            _fail(
                "missing_reference",
                f"tests[{index}].tool_name",
                f"agent {test.agent!r} does not enable tool method {test.tool_name!r}",
            )

    for index, entry in enumerate(tools):
        relative = _authored_relative_path(entry["root"], f"tools[{index}].root")
        tool_root = _safe_package_path(root, relative, kind="directory")
        plugin_relative = relative / ".codex-plugin/plugin.json"
        plugin_file = _safe_package_path(root, plugin_relative, kind="file")
        plugin = _load_json(plugin_file, plugin_relative.as_posix())
        if plugin.get("mcpServers") != "./.mcp.json":
            _fail(
                "invalid_type",
                f"{plugin_relative.as_posix()}.mcpServers",
                "must equal './.mcp.json'",
            )
        mcp_relative = relative / ".mcp.json"
        mcp_file = _safe_package_path(root, mcp_relative, kind="file")
        mcp = _load_json(mcp_file, mcp_relative.as_posix())
        servers = mcp.get("mcpServers")
        if not isinstance(servers, dict):
            _fail(
                "invalid_type",
                f"{mcp_relative.as_posix()}.mcpServers",
                "must be an object",
            )
        if entry["id"] not in servers:
            _fail(
                "missing_reference",
                f"{mcp_relative.as_posix()}.mcpServers",
                f"must contain declared tool {entry['id']!r}",
            )
        descriptor_paths = [
            (plugin_relative.as_posix(), plugin_file),
            (mcp_relative.as_posix(), mcp_file),
        ]
        author_files.update(descriptor_paths)

    digest = hashlib.sha256()
    for relative_path in sorted(author_files):
        contents = author_files[relative_path].read_bytes()
        path_bytes = relative_path.encode("utf-8")
        digest.update(len(path_bytes).to_bytes(8, "big"))
        digest.update(path_bytes)
        digest.update(len(contents).to_bytes(8, "big"))
        digest.update(contents)

    return CopilotPackageSummary(
        id=copilot_id,
        display_name=display_name,
        skill_ids=skill_ids,
        agent_ids=agent_ids,
        tool_ids=tool_ids,
        test_ids=tuple(test.id for test in tests),
        composition_descriptor_sha256=digest.hexdigest(),
    )


def load_copilot_test_cases(
    source_root: Path, manifest_path: Path = Path("copilot.toml")
) -> tuple[CopilotTestCase, ...]:
    """Load tests only after the complete package passes static validation."""

    summary = validate_copilot_package(source_root, manifest_path)
    root = Path(source_root).resolve(strict=True)
    manifest_relative = _manifest_relative_path(root, manifest_path)
    manifest = _load_toml(root / manifest_relative, manifest_relative.as_posix())
    tests = _test_entries(
        manifest, agent_ids=summary.agent_ids, tool_ids=summary.tool_ids
    )
    return tuple(tests)


def _fail(code: str, relative_path: str, message: str) -> None:
    raise CopilotPackageError(code, relative_path, message)


def _manifest_relative_path(root: Path, manifest_path: Path | None) -> Path:
    candidate = Path("copilot.toml") if manifest_path is None else Path(manifest_path)
    if candidate.is_absolute():
        _fail("invalid_path", str(candidate), "manifest path must be source-root-relative")
    return _authored_relative_path(candidate, "manifest_path")


def _authored_relative_path(value: Any, field: str) -> Path:
    if not isinstance(value, (str, os.PathLike)):
        _fail("invalid_type", field, "must be a relative path string")
    raw = os.fspath(value)
    if not isinstance(raw, str) or not raw:
        _fail("invalid_path", field, "must be a non-empty relative path")
    path = Path(raw)
    if path.is_absolute():
        _fail("invalid_path", field, "absolute paths are not allowed")
    if any(part == ".." for part in path.parts):
        _fail("invalid_path", field, "parent traversal is not allowed")
    if path == Path(".") or any(part in ("", ".") for part in path.parts):
        _fail("invalid_path", field, "path must identify a package entry")
    return path


def _safe_package_path(root: Path, relative: Path, *, kind: str) -> Path:
    current = root
    for part in relative.parts:
        current = current / part
        try:
            mode = current.lstat().st_mode
        except FileNotFoundError:
            _fail("missing_file", relative.as_posix(), f"{kind} does not exist")
        if stat.S_ISLNK(mode):
            _fail(
                "unsafe_symlink",
                relative.as_posix(),
                "symlinks are not allowed in package paths",
            )
    if kind == "file" and not current.is_file():
        _fail("invalid_type", relative.as_posix(), "must be a file")
    if kind == "directory" and not current.is_dir():
        _fail("invalid_type", relative.as_posix(), "must be a directory")
    return current


def _load_toml(path: Path, relative_path: str) -> dict[str, Any]:
    try:
        with path.open("rb") as handle:
            value = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        _fail("invalid_toml", relative_path, str(exc))
    if not isinstance(value, dict):  # Defensive; tomllib currently always returns dict.
        _fail("invalid_type", relative_path, "TOML document must be a table")
    return value


def _load_json(path: Path, relative_path: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        _fail("invalid_type", relative_path, f"invalid JSON: {exc}")
    if not isinstance(value, dict):
        _fail("invalid_type", relative_path, "JSON document must be an object")
    return value


def _required(table: dict[str, Any], key: str, relative_path: str) -> Any:
    if key not in table:
        _fail("required_field", f"{relative_path}.{key}", "required field is missing")
    return table[key]


def _required_string(table: dict[str, Any], key: str, relative_path: str) -> str:
    value = _required(table, key, relative_path)
    if not isinstance(value, str) or not value.strip():
        _fail("invalid_type", f"{relative_path}.{key}", "must be a non-empty string")
    return value


def _component_entries(
    manifest: dict[str, Any], section: str, path_field: str
) -> list[dict[str, str]]:
    raw_entries = _required(manifest, section, "copilot.toml")
    if not isinstance(raw_entries, list):
        _fail("invalid_type", section, "must be an array of tables")
    entries: list[dict[str, str]] = []
    seen: set[str] = set()
    for index, raw_entry in enumerate(raw_entries):
        location = f"{section}[{index}]"
        if not isinstance(raw_entry, dict):
            _fail("invalid_type", location, "must be a table")
        entry_id = _required_string(raw_entry, "id", location)
        entry_path = _required_string(raw_entry, path_field, location)
        if entry_id in seen:
            _fail("duplicate_id", f"{location}.id", f"duplicate id {entry_id!r}")
        seen.add(entry_id)
        entries.append({"id": entry_id, path_field: entry_path})
    return entries


def _test_entries(
    manifest: dict[str, Any], *, agent_ids: tuple[str, ...], tool_ids: tuple[str, ...]
) -> list[CopilotTestCase]:
    raw_tests = manifest.get("tests", [])
    if not isinstance(raw_tests, list):
        _fail("invalid_type", "tests", "must be an array of tables")
    tests: list[CopilotTestCase] = []
    seen: set[str] = set()
    for index, raw_test in enumerate(raw_tests):
        location = f"tests[{index}]"
        if not isinstance(raw_test, dict):
            _fail("invalid_type", location, "must be a table")
        test_id = _required_string(raw_test, "id", location)
        if test_id in seen:
            _fail("duplicate_id", f"{location}.id", f"duplicate id {test_id!r}")
        seen.add(test_id)
        prompt = _required_string(raw_test, "prompt", location)
        agent = _required_string(raw_test, "agent", location)
        tool = _required_string(raw_test, "tool", location)
        tool_name = _required_string(raw_test, "tool_name", location)
        if agent not in agent_ids:
            _fail(
                "missing_reference",
                f"{location}.agent",
                f"references undeclared agent {agent!r}",
            )
        if tool not in tool_ids:
            _fail(
                "missing_reference",
                f"{location}.tool",
                f"references undeclared tool {tool!r}",
            )
        arguments = raw_test.get("arguments", {})
        if not isinstance(arguments, dict):
            _fail("invalid_type", f"{location}.arguments", "must be a table")
        expect = _required(raw_test, "expect", location)
        if not isinstance(expect, dict):
            _fail("invalid_type", f"{location}.expect", "must be a table")
        structured = _required(expect, "structured_content", f"{location}.expect")
        if not isinstance(structured, dict):
            _fail(
                "invalid_type",
                f"{location}.expect.structured_content",
                "must be a table",
            )
        _validate_bounded_json(arguments, f"{location}.arguments")
        _validate_bounded_json(
            structured, f"{location}.expect.structured_content"
        )
        tests.append(
            CopilotTestCase(
                id=test_id,
                prompt=prompt,
                agent=agent,
                tool=tool,
                tool_name=tool_name,
                arguments=arguments,
                expected_structured_content=structured,
            )
        )
    return tests


def _validate_bounded_json(value: Any, location: str, *, depth: int = 0) -> None:
    if depth > 4:
        _fail("invalid_type", location, "must not exceed four nested levels")
    if value is None or type(value) in (bool, int, float):
        return
    if isinstance(value, str):
        if len(value) > 2_000:
            _fail("invalid_type", location, "string must not exceed 2000 characters")
        return
    if isinstance(value, list):
        if len(value) > 32:
            _fail("invalid_type", location, "array must not exceed 32 entries")
        for index, item in enumerate(value):
            _validate_bounded_json(item, f"{location}[{index}]", depth=depth + 1)
        return
    if isinstance(value, dict):
        if len(value) > 32:
            _fail("invalid_type", location, "table must not exceed 32 entries")
        for key, item in value.items():
            if not isinstance(key, str) or not key:
                _fail("invalid_type", location, "table keys must be non-empty strings")
            _validate_bounded_json(item, f"{location}.{key}", depth=depth + 1)
        return
    _fail("invalid_type", location, "must contain only JSON-compatible values")


def _skill_frontmatter_name(path: Path, relative_path: str) -> str:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        _fail("frontmatter_mismatch", relative_path, str(exc))
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        _fail("frontmatter_mismatch", relative_path, "missing YAML frontmatter")
    try:
        end = next(index for index, line in enumerate(lines[1:], 1) if line.strip() == "---")
    except StopIteration:
        _fail("frontmatter_mismatch", relative_path, "unterminated YAML frontmatter")
    fields: dict[str, str] = {}
    for line in lines[1:end]:
        if not line or line[0].isspace() or line.startswith(("-", "[", "{", ">", "|")):
            _fail(
                "frontmatter_mismatch",
                relative_path,
                "frontmatter must contain only top-level single-line scalars",
            )
        if ":" not in line:
            _fail("frontmatter_mismatch", relative_path, "invalid frontmatter field")
        key, raw_value = line.split(":", 1)
        if key not in ("name", "description") or key in fields:
            _fail("frontmatter_mismatch", relative_path, "unknown or duplicate field")
        raw_value = raw_value.strip()
        if not raw_value or raw_value[0] in ("[", "{", ">", "|", "&", "*"):
            _fail("frontmatter_mismatch", relative_path, f"{key} must be a scalar")
        if raw_value[0] in "\"'":
            if len(raw_value) < 2 or raw_value[-1] != raw_value[0]:
                _fail("frontmatter_mismatch", relative_path, f"{key} has invalid quotes")
            value = raw_value[1:-1]
        else:
            if " #" in raw_value:
                value = raw_value.split(" #", 1)[0].rstrip()
            else:
                value = raw_value
            if value.startswith(("!", "?")):
                _fail("frontmatter_mismatch", relative_path, f"{key} must be plain text")
        if not value.strip():
            _fail("frontmatter_mismatch", relative_path, f"{key} must not be empty")
        fields[key] = value
    if set(fields) != {"name", "description"}:
        _fail(
            "frontmatter_mismatch",
            relative_path,
            "frontmatter requires name and description exactly once",
        )
    name = fields["name"]
    if len(name) > 64 or any(
        character not in "abcdefghijklmnopqrstuvwxyz0123456789-" for character in name
    ) or name.startswith("-") or name.endswith("-"):
        _fail(
            "frontmatter_mismatch",
            relative_path,
            "name must be 1-64 lowercase letters, digits, or hyphens",
        )
    return name


def _validate_role_references(
    role: dict[str, Any],
    relative_path: str,
    skill_ids: tuple[str, ...],
    tool_ids: tuple[str, ...],
) -> None:
    skills_table = role.get("skills")
    if skills_table is not None:
        if not isinstance(skills_table, dict):
            _fail("invalid_type", f"{relative_path}.skills", "must be a table")
        config = skills_table.get("config")
        if config is not None:
            if not isinstance(config, list):
                _fail("invalid_type", f"{relative_path}.skills.config", "must be an array")
            for index, item in enumerate(config):
                location = f"{relative_path}.skills.config[{index}]"
                if not isinstance(item, dict):
                    _fail("invalid_type", location, "must be a table")
                name = _required_string(item, "name", location)
                enabled = item.get("enabled", True)
                if not isinstance(enabled, bool):
                    _fail("invalid_type", f"{location}.enabled", "must be a boolean")
                if enabled and name not in skill_ids:
                    _fail(
                        "missing_reference",
                        f"{location}.name",
                        f"references undeclared skill {name!r}",
                    )

    mcp_servers = role.get("mcp_servers")
    if mcp_servers is not None:
        _fail(
            "invalid_field",
            f"{relative_path}.mcp_servers",
            "authored Roles must declare Tool policy through plugins; Runtime transport is derived",
        )

    _role_tool_policies(role, relative_path, tool_ids)


def _role_tool_policies(
    role: dict[str, Any], relative_path: str, tool_ids: tuple[str, ...]
) -> dict[str, set[str] | None]:
    """Return native selected-Plugin MCP policy by declared capability-root ID."""

    policies: dict[str, set[str] | None] = {}
    plugins = role.get("plugins")
    if plugins is None:
        return policies
    if not isinstance(plugins, dict):
        _fail("invalid_type", f"{relative_path}.plugins", "must be a table")
    for plugin_id, plugin in plugins.items():
        location = f"{relative_path}.plugins.{plugin_id}"
        if plugin_id not in tool_ids:
            _fail(
                "missing_reference",
                location,
                f"references undeclared tool capability root {plugin_id!r}",
            )
        if not isinstance(plugin, dict):
            _fail("invalid_type", location, "must be a table")
        servers = plugin.get("mcp_servers")
        if not isinstance(servers, dict):
            _fail("required_field", f"{location}.mcp_servers", "required table is missing")
        server = servers.get(plugin_id)
        if not isinstance(server, dict):
            _fail(
                "missing_reference",
                f"{location}.mcp_servers",
                f"must contain the declared MCP server {plugin_id!r}",
            )
        enabled = server.get("enabled_tools")
        if enabled is not None:
            if not isinstance(enabled, list) or not enabled or any(
                not isinstance(item, str) or not item for item in enabled
            ):
                _fail(
                    "invalid_type",
                    f"{location}.mcp_servers.{plugin_id}.enabled_tools",
                    "must be a non-empty string array",
                )
            policies[plugin_id] = set(enabled)
        else:
            policies[plugin_id] = None
    return policies

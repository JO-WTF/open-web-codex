# Hello Agent

This tutorial capability package demonstrates the smallest path from
deterministic Python code to one Tool-using Agent and then two real Codex child
Agents.

## What this package owns

- `hello_agent/core.py` owns the typed greeting contract and deterministic
  Writer/Reviewer rules.
- `hello_writer` and `hello_reviewer` expose those rules as separate MCP
  servers.
- `skills/` tells Codex when and how to call each server.
- `.codex-plugin/plugin.json` groups both Skills and MCP servers into one
  discoverable capability root.

The Plugin does not create an Agent, persist a Thread, publish an enterprise
Agent Definition, or grant permissions.

## Local verification

```bash
tools/hello-agent/bin/setup-env

.local/open-web-codex/tool-envs/hello-agent/bin/python \
  -m pytest tools/hello-agent/tests -q

.local/open-web-codex/tool-envs/hello-agent/bin/python \
  tools/hello-agent/tests/stdio_smoke.py
```

## Runtime Role examples

`examples/runtime-roles/` contains the developer instructions used by the
Hello Team tutorial:

- `greeting-writer.md`
- `greeting-reviewer.md`

Create the Roles through the typed Profile Agent settings before starting a new
root Thread. These files are examples, not automatically discovered Plugin
content, and no setup script writes them into `CODEX_HOME`.

`examples/hello-team-request.md` is the corresponding root Supervisor request.
Both child Threads inherit the root Thread's selected capability roots in the
current Runtime. The Role instructions separate responsibility, but they are
not an authorization boundary.

---
name: __CHILD_SKILL__
description: Execute the focused __DISPLAY_NAME__ worker task with the declared typed tool and return a concise verified result.
metadata:
  short-description: Execute __DISPLAY_NAME__ work
---

# __DISPLAY_NAME__ Worker

Use the declared `__TOOL_ID__` tools for the assigned task.

MCP Tool schemas are deferred. In every new Turn that needs a Tool, use native `tool_search` with the current objective before any MCP Tool call. A Tool name, schema, parameter, or result in history is not a current-Turn callable schema; do not ask a platform layer to search or replay it.

Validate required inputs before calling a tool. Return the typed result and any exact downstream reference the supervisor needs.

Stop with an explicit unavailable or failed outcome when inputs, permissions, or tools are missing.

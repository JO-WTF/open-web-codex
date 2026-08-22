---
name: __CHILD_SKILL__
description: Execute the focused __DISPLAY_NAME__ worker task with the declared typed tool and return a concise verified result.
metadata:
  short-description: Execute __DISPLAY_NAME__ work
---

# __DISPLAY_NAME__ Worker

Use the declared `__TOOL_ID__` tools for the assigned task.

MCP Tool schemas are deferred. When the current request does not expose a Tool required by the current objective, use native `tool_search` with that objective before calling it. A completed client ToolSearchOutput preserved in canonical Thread history and projected into the current request remains callable; do not search again merely because the Turn is new or resumed, and do not ask a platform layer to search or replay history.

Validate required inputs before calling a tool. Return the typed result and any exact downstream reference the supervisor needs.

Stop with an explicit unavailable or failed outcome when inputs, permissions, or tools are missing.

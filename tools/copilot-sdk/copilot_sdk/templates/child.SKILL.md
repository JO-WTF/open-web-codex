---
name: __CHILD_SKILL__
description: Execute the focused __DISPLAY_NAME__ worker task with the declared typed tool and return a concise verified result.
---

# __DISPLAY_NAME__ Worker

Use the declared `__TOOL_ID__` tools for the assigned task.

Validate required inputs before calling a tool. Return the typed result and any exact downstream reference the supervisor needs.

Stop with an explicit unavailable or failed outcome when inputs, permissions, or tools are missing.

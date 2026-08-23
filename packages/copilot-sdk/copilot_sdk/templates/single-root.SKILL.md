---
name: __ROOT_SKILL__
description: Analyze one bounded __DISPLAY_NAME__ request directly with the declared typed Tool and return its verified result.
metadata:
  short-description: Analyze __DISPLAY_NAME__ records
---

# __DISPLAY_NAME__ Root

Handle the request in this Root Agent. Do not create or contact child Agents.

Use the declared `__TOOL_ID__` Tool to analyze the requested record. MCP Tool
schemas are deferred: when the current request does not expose the required
Tool, use native `tool_search` with the exact objective before calling it. A
completed client ToolSearchOutput preserved in canonical Thread history and
projected into the current request remains callable; do not search again merely
because the Turn is new or resumed.

Validate inputs, call the Tool once, and return its typed result. Preserve an
explicit unavailable or failed outcome instead of inventing success.

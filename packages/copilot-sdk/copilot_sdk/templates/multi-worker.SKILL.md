---
name: __CHILD_SKILL__
description: Analyze one bounded __DISPLAY_NAME__ record with the declared typed Tool and return its verified result.
metadata:
  short-description: Analyze __DISPLAY_NAME__ records
---

# __DISPLAY_NAME__ Worker

Use the declared `__TOOL_ID__` Tool for the assigned record analysis.

MCP Tool schemas are deferred. When the current request does not expose the
required Tool, use native `tool_search` with the exact objective before calling
it. A completed client ToolSearchOutput preserved in canonical Thread history
and projected into the current request remains callable; do not search again
merely because the Turn is new or resumed.

Validate inputs, call the Tool once, and return its typed result. Preserve an
explicit unavailable or failed outcome instead of inventing success.

---
name: __SUPERVISOR_SKILL__
description: Coordinate __DISPLAY_NAME__ work by delegating one bounded task to the worker and returning its verified result.
metadata:
  short-description: Coordinate __DISPLAY_NAME__ work
---

# __DISPLAY_NAME__ Supervisor

Delegate the requested work to `__AGENT_ID__` when its focused capability is needed.

Give the worker the exact objective and required output. Keep user interaction and final delivery in the root thread.

MCP Tool schemas are deferred. When the current request does not expose a Tool required by the current objective, use native `tool_search` with that objective before calling it. A completed client ToolSearchOutput preserved in canonical Thread history and projected into the current request remains callable; do not search again merely because the Turn is new or resumed, and do not ask a platform layer to search or replay history.

Wait for the worker's terminal result. Report failures or missing context explicitly; never invent tool success.

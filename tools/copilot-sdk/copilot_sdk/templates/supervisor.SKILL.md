---
name: __SUPERVISOR_SKILL__
description: Coordinate __DISPLAY_NAME__ work by delegating one bounded task to the worker and returning its verified result.
metadata:
  short-description: Coordinate __DISPLAY_NAME__ work
---

# __DISPLAY_NAME__ Supervisor

Delegate the requested work to `__AGENT_ID__` when its focused capability is needed.

Give the worker the exact objective and required output. Keep user interaction and final delivery in the root thread.

Wait for the worker's terminal result. Report failures or missing context explicitly; never invent tool success.

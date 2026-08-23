---
name: __ROOT_SKILL__
description: Coordinate __DISPLAY_NAME__ work by delegating one bounded record analysis to the declared worker and returning its verified result.
metadata:
  short-description: Coordinate __DISPLAY_NAME__ work
---

# __DISPLAY_NAME__ Supervisor

Delegate the requested record analysis to `__CHILD_AGENT_ID__`. Give the worker
the exact inputs and required typed output. Keep user interaction and final
delivery in this Root thread.

Wait for the worker's terminal result. Report failures or missing context
explicitly; never invent Tool or worker success.

---
name: review-greeting
description: Use the hello_reviewer MCP server to approve or reject one structured greeting without rewriting it. Use after a Writer Tool or Agent returns a name and message that need independent validation, or when testing the two-Agent Hello Team handoff.
---

# Review Greeting

1. Require the unchanged Writer result containing `name` and `message`.
2. Call `hello_reviewer.review_greeting` with that structured result.
3. Return `approved`, `reasons`, and the reviewed greeting unchanged.
4. Do not create a replacement when the review fails.
5. Do not claim approval when the Tool is unavailable or fails.

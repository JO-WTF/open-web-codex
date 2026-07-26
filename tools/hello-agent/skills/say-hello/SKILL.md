---
name: say-hello
description: Use the hello_writer MCP server to generate one deterministic structured greeting for a named person. Use when a user asks to greet, welcome, or say hello to one person, or when the Hello Team workflow needs a Writer result for independent review.
---

# Say Hello

1. Require one explicit person's name. Ask for it when missing; never guess.
2. Call `hello_writer.say_hello` exactly once with that name.
3. Return the structured `name` and `message` from the Tool.
4. When another Agent will review the result, pass the unchanged structured result.
5. If the Tool rejects the input or is unavailable, report the failure instead of composing a
   substitute greeting.

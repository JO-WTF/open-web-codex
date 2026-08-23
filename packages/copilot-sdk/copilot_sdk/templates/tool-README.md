# __DISPLAY_NAME__ Tool

This is a root-level shared Tool package. `tool.toml` is its package identity,
`runtime.toml` is the only runtime declaration, and `src/__PYTHON_PACKAGE__/`
owns the typed implementation.

The generated `analyze_record` Tool is deliberately domain-neutral. Replace
its input and result types with your domain contract before adding it to a
Copilot. Do not add Provider SDK dependencies unless the Tool actually owns
typed MCP Resources or authorized Workspace file operations.

Reference it from a Copilot manifest through the explicit Tool registry:

```toml
[[tools]]
id = "__TOOL_ID__"
package = "__TOOL_PACKAGE_ID__"
```

Run the implementation tests from an environment prepared with the hashed
lock file:

```bash
python -m unittest discover -s tests
```

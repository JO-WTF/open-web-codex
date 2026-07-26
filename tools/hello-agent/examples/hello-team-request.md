请用两个真实子 Agent 生成并审核一条给小林的问候：

1. 创建一个 `greeting_writer`，让它调用 Writer Tool 生成结构化问候，然后等待完成。
2. 将 Writer 返回的 `name` 和 `message` 原样交给一个新的
   `greeting_reviewer`，让它调用 Reviewer Tool 审核，然后等待完成。
3. 不允许子 Agent 再创建 Agent，不要用根 Agent 代替缺失的 Role 或 Tool。
4. 只有 `approved=true` 时才返回问候；否则返回审核原因。
5. 最后说明实际创建了哪些 Role，以及每个 Role 调用了哪个 MCP Tool。

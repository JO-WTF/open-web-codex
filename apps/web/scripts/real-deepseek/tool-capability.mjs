// Diagnostic-only scenario. It intentionally asks for the historical D2 path.

export const taskPrompt = [
  "这是一次最小 Provider 能力验证，不执行业务分析。",
  "第一步必须调用原生 tool_search，查询可用的 multi-agent spawn_agent 工具。",
  "tool_search 成功后只调用一次 spawn_agent，然后停止；不要调用 exec_command，不要读取文件。",
].join("\n");

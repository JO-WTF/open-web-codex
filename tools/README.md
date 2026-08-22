# 工具包目录

这里存放可由 Copilot 通过 MCP 运行的领域工具包：

- `warehouse-network-planner`：仓网数据准备、路线、成本、选址和报告。
- `warehouse-network-maps`：仓网 GeoJSON 和交互地图卡片。

`hello-agent` 是开发教程使用的最小 MCP/Plugin 样例，不属于生产仓网链。

通用 Copilot SDK 位于仓库根级 `packages/`，不属于领域工具包：

- `packages/copilot-sdk`：Copilot 包校验、工具环境准备和描述符生成。
- `packages/copilot-provider-sdk`：MCP provider 共用的 Resource 与 Workspace 基础能力。

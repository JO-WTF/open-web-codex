# Warehouse Network Copilot

这是多 Agent 仓网 Copilot 的独立开发者工程。平台不会在 Server 源码中内置这套
Agent 或 Skill；它与单 Agent 仓网 Copilot 只共享根级 Tool package。

```text
warehouse-network/
├── copilot.toml          # 组合清单
├── agents/               # Data / Network Agent Role
└── skills/               # Supervisor 与 Agent Skill

tools/
├── warehouse-network-planner/  # 共享仓网 Tool package
└── warehouse-network-maps/     # 共享地图 Tool package
```

`supply_chain_planner` 只是 Planner 发布后的 Python 导入名，由 `pyproject.toml` 映射到
物理 `src/`；源码树中不再存在第二个同名 Planner 目录。

最终地图文件、中文 Markdown 简报与对话内 GeoJSON 地图卡片都由 `copilot.toml` 的四条
`[[deliveries]]` 精确声明其 MCP producer 和通用交付类型；Platform 不包含仓网 Tool 名、业务
schema 或 marker 的硬编码。

在仓库根目录验证和准备：

```bash
python3 -m pip install -e tools/copilot-provider-sdk -e tools/copilot-sdk
python3 -m copilot_sdk validate copilots/warehouse-network --tool-registry-root tools
python3 -m copilot_sdk prepare copilots/warehouse-network \
  --tool-registry-root tools \
  --output-root "$PWD/.local/open-web-codex/tool-environments/warehouse-network-copilot"
```

修改 Agent 或 Skill 文案会更新组合描述，但不会重装未变化的 Tool 依赖。只有 Tool
源码、依赖声明、锁文件或准备器/解释器身份变化时，才重建 Tool 环境。
`copilot dev` 和 `copilot test` 默认复用 SDK 的本机 Tool 环境缓存，也可以用
`--tool-environment-root` 显式选择与 `copilot prepare` 相同的缓存目录；临时 Profile
和运行状态仍会在命令结束后清理。

Platform 负责通用的验证、依赖准备、Profile 投影和 Runtime 启动；本目录只负责该多 Agent
Copilot 的组合、提示、Role、交付声明与测试。Tool 源码由根级 Tool package 独立拥有。

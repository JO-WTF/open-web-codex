# Warehouse Network Copilot

这是仓网 Copilot 的完整开发者工程，也是定制入口。平台不会在 Server 源码中内置这套
Agent、Skill 或领域 Tool。

```text
warehouse-network/
├── copilot.toml          # 组合清单
├── agents/               # Data / Network Agent Role
├── skills/               # Supervisor 与 Agent Skill
└── tools/
    ├── planner/          # 仓网领域 Tool
    │   ├── src/          # 唯一源码根
    │   │   ├── data/
    │   │   ├── network/
    │   │   ├── delivery/
    │   │   ├── resources/
    │   │   └── shared/
    │   ├── contracts/
    │   ├── examples/
    │   └── tests/
    └── maps/             # 地图 Tool
```

`supply_chain_planner` 只是 Planner 发布后的 Python 导入名，由 `pyproject.toml` 映射到
物理 `src/`；源码树中不再存在第二个同名 Planner 目录。

在仓库根目录验证和准备：

```bash
PYTHONPATH=tools/copilot-sdk python3 -m copilot_sdk validate copilots/warehouse-network
PYTHONPATH=tools/copilot-sdk python3 -m copilot_sdk prepare copilots/warehouse-network \
  --output-root "$PWD/.local/open-web-codex/tool-environments/warehouse-network"
```

修改 Agent 或 Skill 文案会更新组合描述，但不会重装未变化的 Tool 依赖。只有 Tool
源码、依赖声明、锁文件或准备器/解释器身份变化时，才重建 Tool 环境。
`copilot dev` 和 `copilot test` 默认复用 SDK 的本机 Tool 环境缓存，也可以用
`--tool-environment-root` 显式选择与 `copilot prepare` 相同的缓存目录；临时 Profile
和运行状态仍会在命令结束后清理。

Platform 负责通用的验证、依赖准备、Profile 投影和 Runtime 启动；本目录只负责仓网
Copilot 的组合、提示、Role、领域 Tool、合同与测试。

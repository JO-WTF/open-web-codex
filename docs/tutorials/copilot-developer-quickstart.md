# Copilot 开发者快速开始

这个快速开始使用 Copilot SDK Atom 1 创建并静态验证一个最小 Copilot 源码目录。完成后，
你会得到一个声明 Supervisor Skill、child Skill、Runtime Role 和 Tool 组件的 `copilot.toml`。

当前流程不会安装或运行 Copilot。开发循环、测试编排、Profile 安装、Runtime discovery 和
readiness 尚未实现。

## 准备 SDK

需要 Python 3.11 或更高版本。在仓库根目录安装当前 checkout：

```bash
python3 -m pip install -e tools/copilot-sdk
```

## 创建源码目录

```bash
copilot init ./scratch/my-copilot --name my-copilot
```

目标目录必须为空或尚不存在。生成的源码骨架可以立即执行静态验证，但其中的示例组件并不
代表已具备真实业务实现或 Runtime readiness。

## 验证组合

```bash
copilot validate ./scratch/my-copilot
```

第一个参数是显式 source root。manifest 中所有 `path`、`role` 和 `root` 都相对于这个
source root，而不是相对于 `copilot.toml` 所在目录。默认读取 source root 下的
`copilot.toml`。

验证会检查 manifest schema、组件路径、Skill 名称，以及 Role identity 和它对已声明 Skill、
Tool ID 的引用，并返回组合摘要与确定性组合描述 hash；失败会保留具体错误码和相对路径。
这是静态引用/描述合同，不证明完整 Role 配置可被 Runtime 加载；该 Runtime 门属于 Atom 2。

## 验证仓网 monorepo reference

内置仓网 Copilot 的 manifest 位于仓库子目录，但其显式 source root 是仓库根，因为它引用
仓库内其他位置的真实 Tool package。仍在仓库根目录运行：

```bash
copilot validate . \
  --manifest apps/web/builtin/warehouse-network-copilot/copilot.toml
```

这里的 Tool ID 是 `supply_chain` 与 `map_utils`，与两项 Runtime Role TOML 中的
`mcp_servers` key 一致。

## 当前边界

`copilot init` 与 `copilot validate` 是当前 Copilot 开发者入口。已有的
`copilot tool ...` 命令服务于高级 Tool 组件开发，不是这一新手流程的一部分，也不构成
Copilot 运行就绪证据。

Web Settings 中的 Agents 是 Codex Runtime Role 配置，不是 Copilot Builder。当前没有从这里
创建、安装或运行上述源码目录的产品流程。

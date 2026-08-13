# Copilot SDK

Copilot SDK 的 Atom 1 提供一个面向开发者的最小源码入口：创建可组合的 Copilot
源码骨架，并对其 manifest、Skill、Role identity/reference 和 Tool 引用执行静态验证。
它不证明完整 Role 配置可被 Runtime 加载；该 Runtime 门属于 Atom 2。

## 开始使用

需要 Python 3.11 或更高版本。在仓库根目录安装当前 checkout：

```bash
python3 -m pip install -e tools/copilot-sdk
```

创建一个新的 Copilot 源码目录：

```bash
copilot init ./scratch/example-copilot --name example-copilot
```

验证该目录：

```bash
copilot validate ./scratch/example-copilot
```

`init` 生成可以立即通过静态验证的最小组合；这不表示它已经安装到 Profile，或已经具备
开发、测试和 Runtime readiness。

## 显式 source root

`validate` 的第一个参数始终是显式 source root。`copilot.toml` 中的 `path`、`role` 和
`root` 都相对于这个 source root 解析，而不是相对于 manifest 所在目录解析。默认 manifest
是 source root 下的 `copilot.toml`；monorepo 可以通过 `--manifest` 指定 source root 内的
其他位置。

仓库内置仓网 Copilot 是 monorepo reference。请从仓库根目录运行：

```bash
copilot validate . \
  --manifest apps/web/builtin/warehouse-network-copilot/copilot.toml
```

该 reference 声明真实的三项 Skill、两项 Runtime Role，以及 Role TOML 使用的
`supply_chain` 和 `map_utils` Tool ID。

## Atom 1 的边界

当前交付只有源码脚手架和静态组合验证。开发循环、测试编排、Profile 安装、Runtime discovery、
readiness 聚合以及 Web 创作体验尚未实现。已有的 `copilot tool ...` 命令是高级 Tool 组件入口，
不是当前 Copilot 新手入口，也不能证明组合后的 Copilot 可运行。

Settings 中的 Agents 管理 Codex Runtime Role；它不是 Copilot 创建、安装或 readiness 页面。

完整的新手流程见[开发者快速开始](../../docs/tutorials/copilot-developer-quickstart.md)。

# Copilot 开发者中心

> **适合谁**：第一次在本仓库开发 Copilot 的工程师，以及维护现有领域 Copilot 的开发者
> **预计时间**：3 分钟选择路线
> **前置条件**：已克隆本仓库，能在仓库根目录运行命令
> **完成结果**：知道下一篇该读什么，并只使用 `./scripts/copilot.sh` 进入开发流程

这里是仓库内部 Copilot 开发的唯一入口。第一次接触时，先把四个词理解成下面这样：

- **技能（Skill）**是工作方法：告诉 Agent 什么时候做、什么时候问、怎样解释结果；
- **角色（Role）**是权限分工：限定一名 Agent 能用哪些 Skill 和 Tool；
- **工具（Tool）**是确定性执行：校验数据、计算或写文件，成功和失败都有类型化结果；
- **`copilot.toml`** 是装配清单：把 Root、Skill、Role、Tool、测试和交付声明成一个包。

平台负责 Workspace 授权、Task、Codex Runtime 和浏览器展示；Copilot 包只负责自己的业务方法、
角色分工和领域能力。不要把相同业务事实同时写进 Skill、Tool 和平台代码。

## 选择一条路线

| 路线 | 从哪里开始 | 完成结果 |
| --- | --- | --- |
| 第一次开发 | [30 分钟做出第一个 Copilot](first-copilot.md) | 一个业务中性的单 Agent 包，含 typed Tool、两条测试、完整 `check` 和 Web 真调用 |
| 维护真实仓网 | [开发仓网 Copilot](warehouse-copilot.md) | 理解共享 Planner/Maps、单/多 Agent 两个包和三轮地图验收 |
| 查合同或排错 | [概念](concepts.md) → [测试与排错](testing-and-troubleshooting.md) → [参考](reference/cli.md) | 能判断问题属于装配、Role、Tool、Runtime 还是 Web |

第一次开发不要先读架构历史，也不要手改本机配置。主线只有一个命令入口：

```bash
./scripts/copilot.sh --help
```

成功时会列出 `init`、`tool init`、`validate`、`check`、`sync`、`dev` 和 `test`。这个仓库包装器
固定了受信任的共享 Tool 注册表和本地构建位置；教程不会要求设置 `PYTHONPATH`，也不会要求手改
Profile。

## 按需查阅

- [概念：Agent、Skill、Role、Tool 怎样配合](concepts.md)
- [测试与排错](testing-and-troubleshooting.md)
- 参考：[CLI](reference/cli.md) · [`copilot.toml`](reference/copilot-manifest.md) · [`runtime.toml`](reference/tool-runtime.md)
- 进阶：[Skills 与 Roles](advanced/skills-and-roles.md) · [Secrets、审批与外部副作用](advanced/secrets-and-approvals.md) · [交付与 UI](advanced/deliveries-and-ui.md)
- [Provider SDK 可运行 cookbook](../../../packages/copilot-provider-sdk/examples/record-review/README.md)

## 成功信号与失败跳转

成功信号：你能选定上面的一个入口，并且 `./scripts/copilot.sh --help` 正常列出命令。

如果命令不存在、SDK 环境准备失败或 Runtime 二进制缺失，直接跳到
[按阶段排错](testing-and-troubleshooting.md#按阶段排错)，不要绕过包装器安装另一份 SDK。

# Architecture Decision Records

ADR 记录已经接受、会长期约束实现且不能仅从当前代码推断原因的决定。

目标架构报告中的方案比较、开发计划中的任务和 Git 提交不会自动成为 ADR。只有当
一个选择进入实施并需要未来维护者继续遵守时，才创建或更新 ADR。

## 当前记录

| ADR | 决定 |
| --- | --- |
| [ADR-001](001-web-server-framework.md) | 独立 Web Platform Server |
| [ADR-002](002-database.md) | PostgreSQL 与 Migration |
| [ADR-003](003-workspace-structure.md) | 平台 Cargo Workspace 结构 |
| [ADR-004](004-codex-adapter-pattern.md) | Codex Adapter 抽象 |
| [ADR-005](005-map-reply-cards.md) | Inline Visualization Artifact 与回复引用 |
| [ADR-006](006-supervisor-capability-packages.md) | Supervisor Capability Package |
| [ADR-007](007-workspace-dataset-releases.md) | Workspace Dataset Release |
| [ADR-008](008-installable-tutorial-blueprints.md) | 版本化可安装 Tutorial Blueprint |
| [ADR-009](009-thread-first-data-intake.md) | Thread-first 数据接入与分阶段 Readiness |
| [ADR-010](010-workspace-wide-intake-discovery.md) | Workspace 全域数据接入与 Task 级确认 |

## 何时创建 ADR

以下情况通常需要 ADR：

- 改变权威事实所有者或跨层合同；
- 引入难以撤销的数据库、协议或运行时边界；
- 保留新的 Codex 本地 seam；
- 在多个合理方案中选择一个并接受长期代价；
- 改变安全模型、兼容策略或恢复语义。

普通实现细节、可逆重构、当前任务清单和测试结果不需要 ADR。

## 最小结构

每篇 ADR 至少包含：

1. 状态与日期；
2. 背景和需要解决的问题；
3. 最终决定；
4. 被否决的主要替代方案；
5. 后果、风险和验证方式；
6. 被替代时指向新的 ADR。

ADR 描述当时为什么作出决定，可以保留必要历史背景；当前实现状态仍以
[系统架构](../architecture.md) 和 [能力基线](../capability-baseline.md) 为准。

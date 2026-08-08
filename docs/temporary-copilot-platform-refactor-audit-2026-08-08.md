# Copilot 平台重构临时审计

> 文档性质：临时工作底稿，不是当前架构或能力事实的权威来源。
>
> 快照日期：2026-08-08。
>
> 用途：保存本轮仓网 6.0 重构的实际进展、验证证据、未完成项和架构问题，供下一轮平台化设计使用。
>
> 删除条件：正式架构、实施计划和能力基线完成收敛，并且新的真实 E2E 证据已经形成。

## 1. 目标变化

项目当前阶段的目标已经从“跑通一个固定企业案例”调整为：

> 在单用户、单 Profile 阶段建立一个面向未来多用户的平台化多 Agent Web 工作台。
> 算法工程师可以通过 SDK 和 Web 创建、安装和组合 Tool、Skill、Domain Agent 与
> Supervisor，并以较小的场景代码量发布一个可运行、可观察、可恢复的 Copilot。

仓网案例是第一条参考实现，不是平台内部的特殊产品流程。平台能力必须能被其他领域
直接复用，不得要求每个领域重新实现 Agent 协作、输入请求、共享状态、版本锁定、
执行投影和 Artifact 交付。

## 2. 当前已经实现的内容

| 问题 | 当前实现 | 判断 |
| --- | --- | --- |
| `planning-dataset.v2` 成为全流程强前置 | 当前 6.0 包改为按问题准备最小数据，不再声明单体 Dataset Artifact | 根因方向正确 |
| Agent 复制原始表格、矩阵和完整 Tool 结果 | Agent 只交换 `case_id` 与有限摘要；Tool 结果限制为 16 KiB | 明显降低上下文，但尚未实测真实 Token |
| 多个 MCP 入口难以发现和调用 | 当前包只注册 `supply_chain_network`，Agent 使用精确 Tool allowlist | 已通过精确 inventory 测试 |
| Supervisor 写死阶段和 Agent 顺序 | Root 创建同一个 Network Case，根据 readiness 动态委派 Data/Network Agent | 领域行为已动态化 |
| 子 Agent 用户输入不能在 Web 展示 | Runtime 请求先持久化为 Approval，root/child 请求统一归属 root Run，Web 支持多个输入卡片 | 局部测试通过，真实 E2E 未完成 |
| `wait` 刷屏和 Agent 黑盒 | execution projection 保存稳定标题、等待轮次、等待输入和单一终态，普通 wait 不进入主时间线 | 局部测试通过，恢复矩阵未完成 |
| Draft 每次保存要求手工改 semver | Draft 使用整数 revision；发布 Release 时服务器分配版本 | 代码与局部测试完成，数据库并发验证未完成 |
| Mock 被当作失败回退 | Demo Tool 要求显式 demo 意图，空 Workspace 和真实失败不得回退 | Python 测试通过 |
| 仓网确定性能力不足 | 已实现数据标准化、球面矩阵、成本、基线、场景、p-median、时效约束、报告和地图 | Python 边界通过，真实 MCP 链路未完成 |
| DeepSeek 缓存统计恒为 0 | Chat transport 同时解析标准 `prompt_tokens_details.cached_tokens` 和 DeepSeek `prompt_cache_hit_tokens` | 只修复观测，尚未证明真实缓存命中 |

## 3. 已取得的验证证据

- 供应链 Python：95 项测试通过，Ruff 通过。
- Web：188 个测试文件、1333 项测试通过；TypeScript typecheck、ESLint、生产 build 通过。
- Supervisor Catalog：16 项测试通过。
- Approval Service：5 项测试通过。
- Server event projection：26 项通过，1 项需要真实 PostgreSQL 的测试被忽略。
- Codex API：165 项测试通过。
- Tutorial Blueprint 与 Agent/Supervisor 实际内容 hash 已由 Rust 回归测试锁定。
- 教程合同检查通过，只允许一个 Network Case 和两个最终 Artifact 合同。

这些证据证明局部实现成立，不证明新的仓网 Copilot 已完成真实浏览器闭环。

## 4. 当前未完成和阻塞

### P0

1. **真实扩展 E2E 未完成。** 尚未在当前 6.0 实现上验证真实 Data/Network 子 Thread、
   子 Agent 输入、两种 baseline 标签、场景、选址、报告、地图、刷新和恢复。
2. **本地服务当前停止。** 已应用的 `20260807000045` 迁移被开发分支改写，SQLx 正确
   拒绝启动。数据库仍完整；Provider 备份因为本机 `pg_dump 17` 与 PostgreSQL 18
   不匹配而在任何破坏性操作之前停止。
3. **数据 Intake 存在双重所有权。** Platform 已拥有 SourceAsset、DataIntakeSession、
   mapping 和 Dataset Release；Network Case 又保存来源、映射候选与映射选择。
4. **平台事件投影识别仓网专用 Tool envelope。** `event_projection.rs` 显式判断
   `network-case-tool-result.v1`，领域协议泄漏进平台。

### P1

1. Network Case 的 facet、operation、revision、dependency、stale、idempotency 和
   deliverable 关系是通用工作状态能力，却全部实现在供应链 Python 包内。
2. Root 仍通过子 Agent 消息了解大部分业务进度。它缺少平台提供的有界、只读、可信
   collaboration status 工具。
3. Agent assignment 仍主要依赖自然语言约定，没有平台统一的目标、共享状态、输入依赖、
   完成条件和交付件合同。
4. Agent、Supervisor 和 Tutorial Blueprint 的发布 hash 仍需手工同步；本轮已经出现
   Blueprint hash 漂移。
5. 当前只修复了缓存命中计数的解析，没有形成每次模型调用的输入、输出、缓存、延迟、
   Tool schema 占用和 compaction 观测。

### P2

1. `supply_chain_planner/server.py` 仍保留大量不再注册的旧 Resource、snapshot、legacy
   report 和旧 solver 代码；没有完成无兼容负担删除。
2. `docs/roadmap.md`、旧扩展教程和部分当前文档仍描述 2.x/5.x、十五个 Artifact handoff
   与 `planning-dataset.v2`，当前事实存在漂移。
3. 导航矩阵只有规划与费用确认，没有完成当前单一 MCP 下的导航结果注册闭环。
4. `test-web-rust.sh`、数据库并发/恢复矩阵、真实 stdio smoke 尚未全部执行。

## 5. 重构中发现的错误公共化方向

### 5.1 不应把 Network Case 直接升格为平台通用对象

仓网的需求、仓库、路线、成本、覆盖、场景和选址都是领域语义。平台若直接理解这些
字段，会变成供应链专用系统。

### 5.2 应提取 Network Case 背后的通用机制

以下机制不属于仓网：

- Task 范围的共享工作状态身份；
- facet/component revision；
- operation 稳定身份、幂等和完整终态；
- component dependency 与失效；
- readiness、blocking issue 和下一动作摘要；
- Agent 只拿到有界状态，不复制大型 payload；
- 最终 deliverable 与 Artifact 的来源关系。

正式架构应把这些机制定义为通用 Work State；领域包只注册 schema、依赖和 mutation。

### 5.3 已经属于平台的能力不能在领域包重建

- Workspace 文件上传和 SourceAsset；
- 文件 revision 和内容 hash；
- 通用字段映射确认；
- 用户输入、Approval 和 secret 处理；
- Agent execution 生命周期投影；
- Artifact 身份、授权、来源和浏览器渲染；
- Draft revision、Release 和依赖锁定。

## 6. 下一轮架构必须回答的问题

1. Tool、Skill、Agent、Supervisor 和完整 Copilot 分别是什么可发布资源？
2. 开发者如何通过 SDK 创建 Tool，并通过受控安装路径让 Runtime 正式发现？
3. Web 如何编辑自然语言指令，同时不把 Prompt 当作权限和能力真相？
4. Root 如何只读观察子 Agent、共享状态、待输入和交付件，而不依赖子 Agent 自述？
5. Domain Agent 如何共享大型业务状态，同时不建立第二个 Thread、Memory 或 Scheduler？
6. Platform Data Intake 与领域 normalizer 如何连接而不双写？
7. Runtime discovery、平台 Catalog 和已发布 Release 如何区分并在启动时收敛？
8. 开发者如何不手填版本、hash、Runtime Role、MCP 内部名称和宿主路径？
9. 当前官方 Codex API 不足时，最小必要 seam 是什么，如何退出？
10. 一个新的业务 Copilot 如何复用公共测试与真实 E2E harness？

上述问题的稳定答案进入正式架构和实施计划；本文件不继续承担长期设计。

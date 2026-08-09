# ADR-017：以干净 Copilot 主干替换仓网原型边界

状态：已接受（2026-08-08）；已被 [ADR-018](018-built-in-network-copilot-runtime-closure.md)
替代（2026-08-08）。

## 适用范围与权威

本 ADR 是当前单用户、单 Profile Copilot 平台后续设计与开发的接受基线。它不替代：

- `product-vision.md` 的长期产品方向；
- `product-design.md` 的 V1 用户要求；
- `architecture.md` 与 `capability-baseline.md` 的当前实现事实；
- `development-plan.md` 的近期执行任务。

它约束上述文档与后续实现如何解释当前仓网纵向原型、如何划分 Platform、Runtime、
Data 与 Domain owner，以及哪些旧路径必须在新主干完成后删除。若后续决定改变这些
边界，必须以新的 ADR 显式替代本 ADR。

## 背景

当前分支已经取得一条真实最小仓网 E2E 证据：真实 Codex Runtime 与真实 Provider 下，
Root、Data Agent、Network Agent 完成协作，官方用户输入得到解决，Work State 组件与
最终报告形成，测试后置检查通过。

这条证据证明“预装并预接好的仓网 Copilot 可以运行”，但没有证明算法工程师可以通过
SDK 与 Web 从零完成：

```text
Tool -> 中文 Skill -> Domain Agent -> Supervisor -> Copilot
     -> Profile Installation -> Runtime discovery -> Task
```

当前 E2E 仍由脚本直接创建 Work State、复制仓库内 Agent/Supervisor 定义，并在 Prompt
中注入 Run ID、Work State ID、工具名、字段形状、执行顺序和禁止事项。开发过程中已经
出现以下重复失败：

- Workspace `source_ref` 被当作 MCP Resource URI；
- child Agent 尝试调用仅 Root 可用的 `request_user_input`；
- `mappings/entities`、camelCase/snake_case、实体名和字段别名漂移；
- Agent 直接管理 Work State operation、revision、idempotency 和 deliverable mutation；
- Root 过早结束后由 Platform 注入仓网专用 continuation Prompt；
- MCP 旧进程、旧二进制、手工 hash 和 Blueprint 内容不一致；
- 临时 Profile 隐式继承宿主认证并污染 Provider 身份。

这些问题共同说明：概率模型正在承担协议适配、事务和权限职责；Event Projection 正在
承担工作流职责；Platform Data Intake、Work State 与供应链 Case 存在重复 owner。
继续增加 Prompt、aliases、重试或领域投影分支，只会扩大历史包袱。

## 当前阶段裁决

当前不是目标阶段已经顺序推进到仓网迁移，而是：

- Gate 0 基线曾经跑通，但 Profile、进程和安装 generation 仍需收敛；
- Work State、Root Coordination、execution、Approval 与 Provider metrics 有局部实现；
- Data Intake、Assignment、Catalog/Compiler/Installation 尚未形成生产主链；
- SDK 与 Web Studio 是彼此断开的原型；
- 仓网最小 E2E 提前成立，但旧 Case/Data 生命周期尚未删除；
- 第二领域与多用户隔离尚未开始。

因此当前整体处于“公共平台边界收敛期”，不能用仓网 E2E 代替 Copilot 创作平台验收。

## 决定一：产品只有一条 Copilot 创作主线

算法工程师的产品心智模型固定为：

```text
Tool -> Skill -> Agent -> Supervisor -> Copilot
```

Web 采用独立 Copilot Studio，而不是在 Settings 中维护多套目录和编辑器。SDK 负责本地
Tool 开发和测试，Web 负责导入、审查、组合、测试、发布、安装和运行。两者必须消费
同一 package format 与同一 Catalog/Compiler 合同。

普通用户界面不得要求维护：

- semver、content hash、execution hash；
- Runtime Role、MCP server 内部名称或宿主路径；
- Work State ID、Run ID、operation ID 或 CAS revision；
- Plugin/MCP 原始 JSON、Artifact producer/consumer 内部 ID。

用户只看到业务目标、团队成员、能力、输入、权限、交付、测试结果和
`released/installing/installed/discovered/ready` 的可解释状态。错误必须说明失败阶段、
原因、是否可重试和下一步，不把“查看服务端日志”作为普通恢复路径。

## 决定二：采用三平面、模块化单体

当前阶段不拆分微服务。以下 Service/Controller 名称表示一个 Platform Server 中的逻辑
owner 与模块边界：

| 平面 | 唯一 owner | 逻辑模块 |
| --- | --- | --- |
| Control Plane | Platform | Identity/Auth、Catalog/Compiler、Installation、Task/Run、Assignment Grant、Approval、Audit、Artifact metadata |
| Runtime Plane | Codex Runtime；Profile Host 只桥接 | Thread/Turn/context、Root/child Agent、spawn/wait/follow-up、Skills/Plugins/MCP discovery、Tool execution、Provider transport、官方用户输入 |
| Data Plane | Platform Data Intake + Domain Package | SourceAsset、Mapping Revision、Dataset Release、Work State、Domain Resource、Artifact blob；领域 schema/算法/校验 |

关键唯一 owner：

1. `Capability Catalog + Compiler` 是唯一 Draft/Release、版本、hash、依赖锁和 Runtime
   bundle 生成者；代码 seed、Web Draft 与 Tutorial 不能另算 hash。
2. `Installation Controller` 持有 desired state；Profile Host 执行 staging、hash 校验、
   原子激活和 reload；Runtime discovery 提供可执行证据。
3. `Assignment Service` 编译不可变 Assignment Grant，但不 spawn Agent；Codex Runtime
   继续执行真正的 spawn、wait、follow-up 和 interrupt。
4. `Run Completion Controller` 属于 Run Orchestrator，管理等待 children、等待综合和
   恢复；Event Projector 只做幂等投影和广播。
5. `Data Intake` 完整拥有 SourceAsset -> Mapping Revision -> Dataset Release 生命周期。
6. `Work Coordination` 只维护 component、dependency、operation、blocker 与 deliverable
   引用，不理解仓库、路线、成本等领域字段，不成为 Workflow、Memory 或 Blackboard。
7. `Domain Package` 只保留领域 schema、算法、业务校验、normalizer、报告和 renderer。

## 决定三：平台机制、Skill 与 guardrail 分工

| 问题 | Platform/Runtime 硬约束 | Skill/Instructions | 执行 guardrail |
| --- | --- | --- | --- |
| Agent 可调用能力 | 精确 Agent Release、Role、Plugin/MCP/tool allowlist | 何时使用哪个已授权 Tool | 未授权 Tool 不可见 |
| 数据访问 | Assignment Grant、typed handle、scope 与状态校验 | 如何理解证据、何时认为数据不足 | resolver 拒绝错误类型和越权 |
| 用户输入 | 官方 Runtime Root-only | Supervisor 决定何时询问、如何解释 | child 返回 typed `needs_input`，Root 收集后重新派发 |
| Work State | SDK 自动 begin/commit/idempotency，write-set 与 schema 固定 | Agent 只判断业务 outcome | 模型不传 revision、operation ID 或通用 mutation JSON |
| Wire shape | Compiler 生成一个当前 Schema | Skill 不描述大小写和兼容字段 | 严格校验，禁止 aliases 与字段猜测 |
| Run 完成 | required assignments、终态和 deliverable completion gate | Supervisor 综合、解释冲突和部分成功 | 不满足合同不能伪造 completed |
| 安装可用 | Release、Installation、Discovery、Readiness 分别持有事实 | 不属于 Skill | `installed != discovered != ready` |
| 重试 | 稳定 operation identity；只重试显式幂等操作 | 是否更换业务方法 | 有界次数，保留原始 cause |

可信设计只保护不可由模型可靠承担的硬不变量。业务判断、分析方法和沟通方式继续由
Skill/Supervisor 指导。不得建设复杂信任分、证据图、自动重规划服务或通用工作流 DSL
作为当前闭环前置条件。

## 决定四：统一类型化引用

废除 `source_ref`、`data_ref`、MCP Resource URI、Artifact ref 在模型消息中的字符串
方言，使用有 discriminator 的当前合同：

- `SourceAssetRef`：Platform Data Intake 私有输入；仅获授权的数据准备能力可消费；
- `DatasetReleaseRef`：不可变标准化数据；Domain Tool 通过服务端 resolver 消费；
- `DomainResourceRef`：矩阵、方案、求解结果等领域中间资源；按 producer/consumer grant；
- `ArtifactRef`：用户可见、可授权下载或渲染的长期交付物；
- `McpResourceUri`：只允许留在 Runtime/MCP 传输内部，不进入 Work State 或跨 Agent
  assignment。

`source_ref` 不能依赖 Skill 文字禁止误用。Network Agent 不获得 SourceAsset capability；
Data Agent 只调用接受 `SourceAssetRef` 的数据工具；Domain Tool 直接接受
`DatasetReleaseRef`。错误 variant 在 Tool schema/resolver 边界返回 typed `rejected`，
不得根据字符串、路径或错误文本推断。

## 决定五：最小 Root-led 协同协议

协同主线固定为：

1. Root 读取只读 Coordination projection；
2. Root 选择精确 Agent Release 与业务目标；
3. Platform 编译 Assignment Grant：objective、read-set、write-set、exact capabilities、
   expected outputs、parameters 和 budgets；
4. Codex Runtime 执行 `spawn_agent`；
5. Tool Runner 获得服务器注入的 assignment context，模型不提供 Run/WorkState/hash；
6. Tool/SDK 原子提交 `succeeded | needs_input | failed | rejected | cancelled | timeout |
   interrupted` 和类型化引用；
7. `needs_input` 是 child 的终态，Root 使用官方 `request_user_input`，取得答案后创建新
   assignment；
8. 所有 required child terminal 后，Run 进入 `awaiting_synthesis`，由 Root 综合并结束。

状态机至少包含：

```text
prepared -> spawned -> running
                    -> succeeded
                    -> needs_input
                    -> failed | rejected | cancelled | timeout | interrupted
```

Root 过早结束时，Run Orchestrator 可以执行一次通用、有界、幂等的 synthesis 恢复；
恢复内容只要求读取当前 coordination 状态并完成综合，不能包含仓网字段、映射数量或固定
业务步骤。第二次仍未完成则显式 `orchestration_incomplete`，由 UI 提供恢复入口，禁止
无限续跑。

## 决定六：冻结原型并建设 Clean Spine

当前仓网提交保留为算法、失败模式和真实运行证据，不再接受为了维持旧 E2E 而增加：

- 新 wire aliases 或 canonicalizer 分支；
- 领域 continuation Prompt；
- 路径扫描、旧版本 fallback 或隐式 Profile 状态继承；
- 模型可见的低层 Work State mutation；
- Platform 中新的供应链名称、schema 或 Tool 分支。

下一条开发主线必须先完成一个无 Data Intake、无供应链字段的干净垂直切片：

```text
Tool SDK
-> Catalog/Compiler
-> Profile Installation
-> Runtime Discovery
-> 中文 Skill
-> Agent
-> Supervisor/Copilot
-> 两 Agent Run
-> needs_input / Root input / synthesis
```

该切片完成后，再接入 Platform Data Intake、类型化引用和仓网算法，最后用第二领域验证
扩展点。当前阶段所有逻辑模块继续运行在模块化单体中。

## 保留与删除

### 保留

- `codex-rs` 与 Patch Map 中现有 retained seams；
- Profile Host typed bridge、进程生命周期、Runtime capability manifest 和安全 Role 物化；
- Run lease、delivery uncertainty、恢复骨架；
- Provider Service 与加密 Secret Store，但删除隐式宿主认证导入；
- Approval、用户输入、Agent execution、Artifact 的纯投影能力；
- Work State 内部事务、revision、幂等和完整终态机制；
- 仓网领域 schema、算法、报告、地图、fixture 和真实 E2E 证据。

### 新主干完成后删除或替换

- Event Projection 中的 Supervisor continuation、领域 Prompt 和 Data Intake 全局 interrupt；
- `supervisor_run_continuations` 持久化路径；
- `supervisor-catalog`、capability template、旧 Agent/Supervisor routes 与 UI；
- cwd、Workspace、source repo 下的 capability root 扫描；
- Catalog 中无实际物化却标记 `installed` 的路径；
- 模型可见的 `begin/apply/fail Work State` 通用事务 Tool；
- `CaseRepository` 的 source/mapping/revision/operation/readiness/dependency 基础设施；
- Data Server 的 Workspace 扫描、Resource template 和 wire-shape aliases；
- 手工 6.0 版本目录、Runtime Role、Blueprint/package hash 常量；
- 默认从宿主 `$HOME/.codex/auth.json` 导入 Profile 认证；
- Settings 中互相竞争的 Studio 和要求编辑原始 JSON/Python 的产品入口。

切换时更新所有调用方、fixtures、tests、migrations 和 current-state 文档，然后一次删除
旧路径；不做双读、双写、运行时字段猜测或旧项目合同兼容。

## 验收门

Clean Spine 只有同时满足以下条件才完成：

1. 从干净数据库和干净 Profile 开始，不继承宿主 auth、skills、plugins 或 MCP 状态；
2. 算法工程师只用正式 SDK 与 Web 完成 Tool -> Skill -> Agent -> Supervisor/Copilot；
3. 用户不输入版本、hash、Runtime Role、MCP 名称、路径或 Work State/Run ID；
4. `released/installed/discovered/ready` 可独立失败并在 Web 得到安全诊断；
5. Runtime discovery 而不是数据库状态、目录存在或路径扫描决定可执行性；
6. Agent 无法看到或调用未授权 Tool，错误引用在模型执行前或 Tool 边界被拒绝；
7. child `needs_input`、Root 官方输入、重新派发和最终 synthesis 形成可恢复闭环；
8. 浏览器刷新与 Profile 重启后安装、任务、输入和 execution 状态收敛；
9. E2E 从 SDK/Web 用户入口开始，不脚本手建 Work State，不在 Prompt 注入内部 ID；
10. Platform Server 中不存在针对参考领域的名称、Tool 或 schema 分支；
11. 第二领域只新增 package、Skills、Agents、Supervisor 与 renderer；若修改平台领域分支，
    架构验收失败；
12. 所有变更遵守无兼容切换，并更新能力基线和开发计划。

## 被否决的方案

- 继续围绕当前仓网 7/7 E2E 增加 Prompt、aliases、重试和 continuation；
- 把 Network Case 直接提升为 Platform 通用对象；
- 让 Event Projector 或 Platform Server 编排 Agent 工作流；
- 让模型构造授权、Resource URI、operation、revision 或通用 mutation；
- 先建设多微服务、通用工作流 DSL、Blackboard、复杂信任评分或 Marketplace；
- Catalog 发布后直接宣称 Runtime ready；
- 保留新旧 Catalog、Case、Resource 或 Studio 双轨运行。

## 后果

- 当前仓网原型不会被继续包装成最终平台；部分已有 UI、routes、migrations 和领域状态代码
  将被删除。
- 新主干必须先验证通用创作与安装链，短期内不继续扩充仓网成本、场景和地图闭环。
- 硬约束从 Prompt 移到生成合同、能力可见性、服务端上下文和 Tool schema，模型调用成功率
  预期提高，同时越权和 wire drift 更早失败。
- 当前单用户实现继续保留 organization/user/profile/workspace/task/run scope；两用户隔离
  矩阵通过前不开放多用户产品入口。
- 本 ADR 收敛 ADR-013 至 ADR-016：Data Intake、安装和只读 CollaborationContext 的
  专项理由继续有效；ADR-015 的具体 Tool Result wire shape 被本 ADR 的 ToolOutcome 与
  SDK/server 内部 Work State commit 替代；ADR-016 的 Assignment 由本 ADR 扩展为 Grant、
  Root-only input 和 Run Completion。

## 证据入口

- 当前事实与验证等级：`docs/capability-baseline.md`
- 当前和下一里程碑：`docs/development-plan.md`
- Copilot 目标架构：`docs/supervisor-agent-skill-tool-architecture.md`
- M2 owner、合同、删除门与工作包：`docs/agent-capability-lifecycle-plan.md`
- 仓网迁移输入：`docs/enterprise-supervisor-copilot-plan.md`
- 当前代码图谱：`.ua/knowledge-graph.json`；图谱用于结构导航，新增文件仍需以 HEAD 源码
  复核。

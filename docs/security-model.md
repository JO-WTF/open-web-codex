# open-web-codex 安全模型

| 字段 | 内容 |
| --- | --- |
| 文档性质 | 规范性安全边界与发布门禁 |
| 适用范围 | Browser、Platform、Profile Host、Codex Runtime、Workspace、Git、Artifact、MCP |
| 当前验证状态 | 以 [能力基线](capability-baseline.md) 为准 |
| 产品要求 | [产品设计](product-design.md) |
| 系统所有权 | [系统架构](architecture.md) |

本文定义任何产品阶段都不能削弱的安全属性。它不因为源码中存在检查就宣称安全能力
已经通过；真实实现、负向测试和发布证据仍由能力基线记录。

## 安全目标

平台必须保证：

1. 用户只能发现、读取和控制自己被授权的资源；
2. Profile、Thread、Workspace、Secret、Artifact 和事件不会跨用户或组织串流；
3. 浏览器输入不能成为可信服务器路径、Runtime 身份或授权决定；
4. Codex 和子 Agent 不能通过 Prompt、自报身份或 Tool 参数扩大权限；
5. 凭据只在拥有其生命周期的服务端进程或企业 Tool 边界中出现；
6. 审批、取消、恢复和交付具有稳定身份、明确终态和不可抵赖审计；
7. 页面刷新、断线、重启、并发和乱序不会恢复出更高权限或陈旧决定；
8. Runtime 能力未知、未验证或不兼容时失败关闭，不由平台 fallback 模拟。

安全模型不承诺阻止所有模型错误。它要求模型错误不能绕过确定性的身份、权限、
资源和执行边界。

## 保护资产

| 资产 | 主要风险 |
| --- | --- |
| 用户 Session 与组织身份 | 冒用、固定会话、跨组织切换错误 |
| Profile Home / `CODEX_HOME` | Thread、Memory、Provider 和扩展状态泄漏 |
| Provider 与 Git Secret | 浏览器泄漏、日志泄漏、跨 Profile 注入 |
| Workspace 与仓库内容 | 路径穿越、越权读取、错误删除、分支破坏 |
| Thread/Turn 上下文 | 被平台复制、跨用户恢复、错误绑定 |
| Approval 与 Control Lease | 重放、并发双决策、旧进程请求复用 |
| Artifact | 猜测 ID、跨任务读取、内容类型混淆、过期引用 |
| 企业数据与 MCP | Tool 越权、SSRF、调用者身份伪造、权限放大 |
| 事件、审计和评价 | 丢失、乱序、伪造成功、无法追责 |
| Codex 构建与 Provider transport | 供应链污染、能力漂移、协议误判 |

## 信任边界

```text
Untrusted Browser
  -> authenticated typed Platform API
  -> authorization + durable platform state
  -> Profile Host / typed Codex adapter
  -> Codex app-server / Runtime
  -> sandboxed tools and approved MCP connections

Platform
  -> authorized Workspace / Git resources
  -> encrypted Secret store
  -> authorized Artifact store
  -> enterprise Tool/MCP gateway
```

### 浏览器

浏览器是不可信输入端。它可以提交稳定平台资源 ID 和业务参数，但不能提交可信的：

- 服务器本地路径或 `CODEX_HOME`；
- `organization_id`、`profile_id`、AgentPath 或 Runtime Role 身份；
- app-server request ID 或原始 JSON-RPC；
- Secret 明文或配置文件路径；
- 任意 Artifact 存储 URI；
- 声称已经完成的审批、执行或能力状态。

浏览器只接收有界、类型化、脱敏的平台 DTO。

### Platform

Platform 是身份、授权、Task/Run、审批、Artifact、审计和浏览器合同的信任边界。
它可以调用 Runtime，但不能伪造 Thread 历史、模型上下文、Agent 状态或 Tool
执行结果。

### Codex Runtime

Codex 拥有模型可见上下文和执行语义，但不是企业授权系统。Runtime 提出的 Tool
调用、Agent 委派和资源引用必须经过平台或企业 Tool 边界的确定性检查。

### Workspace、Git 与外部系统

Workspace、Git remote、MCP Server、企业 API 和模型 Provider 都位于独立外部
边界。连接成功不等于调用已经获得业务授权；每次资源访问仍需校验当前主体、范围和
动作。

## 身份与授权链

任何资源操作必须能够沿以下链路解析，不能只凭最终资源 ID 命中：

```text
session
  -> user
  -> organization membership
  -> project permission
  -> profile / workspace grant
  -> task / codex thread
  -> run / approval / event
  -> artifact grant + producer provenance
```

### 身份原则

- 身份来自认证 Session 和服务端记录，不来自模型或浏览器参数；
- 用户默认只绑定自己的持久 Profile；
- Platform Admin 的基础设施权限不自动获得业务 Prompt、Thread 或 Artifact 读取权；
- Control Lease 只决定谁能向 Task 发送控制操作，不替代 RBAC；
- Runtime Agent 的 AgentPath 和 Runtime Role 是执行事实，不自动成为企业数据身份；
- 执行授权上下文不能被模型自由填写或转发。

### 授权响应

为避免资源枚举，跨租户或无权访问的资源默认返回不可区分的拒绝结果。审计记录可以
保存内部原因，但浏览器错误不能暴露另一个组织、Profile、路径或对象是否存在。

## Profile 与 Runtime 隔离

- 每个用户的 Profile 拥有独立、持久 `CODEX_HOME`；
- 同一 Profile 同时最多有一个主 app-server 进程；
- Profile 进程、缓存、Provider 目录、Secret 注入和 Runtime request 映射必须以
  Profile 与进程实例为 key；
- Runtime 重启后，旧进程的 request ID、审批和响应能力立即失效；
- Thread 必须属于当前授权 Profile；平台不能通过复制数据库事件恢复模型上下文；
- 多 Profile 路由完成前，单 Profile 模式只能作为受限部署形态，不能宣称多用户
  进程隔离已经成立。

## Workspace 与 Git 安全

Workspace 是独立授权执行根，不属于 Thread、Task 或 Run。

必须满足：

- 浏览器使用 Workspace ID，服务端解析规范化根路径；
- Thread 当前 `cwd` 必须包含在授权 Workspace 内；
- 文件、Terminal、Git、GitHub 和 Runtime 操作分别重新检查 Workspace grant；
- symlink、`..`、嵌套 Git root 和路径规范化不能逃逸授权根；
- 托管 clone/worktree 只能通过显式 Workspace 生命周期创建和删除；
- Run 取消、失败、租约过期或恢复不能隐式删除 Workspace；
- 删除前检查活动 Run、Thread 使用、Terminal、子 worktree 和未交付变更；
- Push、保护分支和远端变更始终需要显式产品操作，禁止 Force Push。

共享 Workspace 会引入并发写入和状态竞争。平台必须通过锁、策略或明确冲突结果
处理，不能通过重新引入“每 Run 私有 Workspace”掩盖问题。

## Secret 与 Provider

- Secret 以平台身份和作用域加密，数据库只保存密文与安全元数据；
- 主密钥由外部 Secret Manager 提供，不能提交到源码、文档或日志；
- Profile Secret 只注入对应 Profile 进程的私有环境；
- 企业 Tool 凭据只注入拥有资源授权判断的服务端边界；
- 浏览器、Artifact、Agent Definition、Supervisor Policy、Prompt 和审计不保存
  Secret 明文；
- Provider 模型目录和选择按 Profile/Provider 隔离，切换时不能复用其他
  Provider 的模型或凭据缓存；
- Secret 删除、轮换和进程替换必须处理活动 Turn、未决 Server Request，以及尚未
  产生首个官方 rollout 的持久 Thread；不能为了立即生效破坏运行一致性或留下无法
  恢复的 Thread 身份。

## Approval、Lease 与异步操作

每个异步操作需要稳定身份和明确终态。

### Approval

- Runtime 请求先持久化和脱敏，再通知浏览器；
- 决策校验用户权限、资源归属、版本和 Runtime 进程实例；
- 根 Thread 与授权 Agent 树中的子 Thread 使用同一任务级审批投影；浏览器只接收平台
  审批 ID、安全动作摘要和 Agent 展示身份，不接收 Runtime request ID；
- 只有全部 Tool 都是只读、有界或确定性计算的已评审 MCP Server，才可在 Plugin
  合同中声明默认预批准，并仍受 Thread capability root 与 Agent Tool allowlist 限制；
  混合风险 Server、凭据、外部副作用、权限扩大和非幂等写入不得使用该默认值；
- 并发决策只有一个成功；
- approved、rejected、expired、cancelled 等终态不可逆；
- 旧进程 request ID 不能在重启后重新获得响应能力；
- `delivery_unknown` 只允许相同决定重试，不允许改变答案。

### Control Lease

- Lease 使用数据库时间、TTL 和版本；
- 同一 Task 同时最多一个有效控制者；
- 强制接管需要额外权限、原因和审计；
- Lease 失效后，浏览器草稿或缓存不能继续发送 Runtime 指令。

### Run

- Run 是调度和审计尝试，不是授权主体；
- lease、heartbeat、取消和恢复必须收敛到明确状态；
- 审批等待属于独立对象，不能由单个 Run 状态字符串替代；
- 恢复不能自动获得新的 Workspace、Profile 或 Artifact 权限。

## Artifact 安全

目标 Artifact 具有独立身份、授权和保留周期。

- Run、Thread、Turn、Item 只记录 provenance；
- Artifact ID 不能包含存储路径或可猜测权限信息；
- 注册时校验 Schema、大小、类型、来源和内容安全策略；
- 浏览器通过授权 DTO 或短时受控 URL 读取；
- 子 Thread、后继 Run 和历史恢复分别执行 Artifact 授权；
- renderer 只处理声明并允许的类型，不执行 Artifact 中的任意脚本；
- 大型 Resource 通过有界读取和授权引用加载，不能把内部 URI 暴露给模型回复；
- 删除、保留、替代和依赖失效产生审计。

当前 Inline Visualization 的 Run/Thread 作用域是能力缺口，不是目标安全边界；在
持久 Artifact 授权完成前，不能宣称支持任意跨 Run 成果共享。

## 多 Agent 与企业 Tool

多 Agent 会放大现有权限，因此每次委派都要保持最小能力：

| 风险 | 必须的控制 |
| --- | --- |
| Supervisor 选择未发布或漂移的 Agent | 候选来自组织可见 Agent Catalog；用户 Agent 必须绑定代码评审的 capability template，Supervisor 必须绑定精确 Release UUID、版本和内容哈希，预检重新解析且不按名称回退 |
| 浏览器扩大 Agent 权限 | 浏览器不能提交 Runtime Role、MCP、Tool、capability root 或路径；服务端只允许收窄所选模板的 Artifact 合同并派生可执行配置 |
| 子 Agent 继承全部权限 | 受限连接、最小凭据和 Capability/Resource 裁剪 |
| 模型伪造 Task 或角色 | 身份由系统绑定，不接受模型提交的组织/Profile/Runtime Role |
| Agent 之间复制敏感数据 | 通过授权 Artifact/Resource 引用交接，避免复制无界上下文 |
| 不同 Agent 结论冲突 | Supervisor 负责补充调查和最终综合，保留各自产物与依据 |
| Agent 网络无限扩张 | Runtime 并发、深度、预算、超时和停止规则 |
| Prompt 注入影响权限 | Prompt 只提供行为指导，服务端继续执行确定性授权 |

在 Runtime 到 MCP 边界能够携带或推导不可伪造 Agent 执行身份之前，平台只能承诺
已经验证的 Task/Profile 级受限资源范围，不能提前宣称 Runtime Role 级动态授权。

## 事件、审计与恢复

- 平台事件和审批先持久化，再向浏览器广播；
- 每个 Task 的 durable event 使用单调 sequence，重连只补缺口；
- 浏览器缓存和 WebSocket 是投影，不是权威历史；
- Thread/Turn 恢复以 Codex Profile 为准；
- 审计包含主体、动作、资源、安全结果、时间和必要版本，不保存 Prompt、代码、
  Secret 或无界 Runtime payload；
- 失败、拒绝、取消、超时、恢复和部分成功必须可区分；
- 备份恢复后重新执行授权检查，不能因为记录存在就恢复旧权限。

## 供应链与 Codex 定制

- Codex 构建固定到明确 revision；
- 生成 Schema、TypeScript、Capability Manifest 和 fixtures 是协议事实；
- 本地非生成差异必须进入 Patch Map，说明原因、测试和退出条件；
- 官方同步在专用分支完成，先接受上游结构，再重放保留 seam；
- 依赖、Plugin、Skill 和 MCP 包需要来源、完整性和权限策略；
- 未验证 Capability 默认关闭，不能由浏览器或 Server 静默 fallback。

## 发布门禁

涉及安全或授权的发布至少需要：

1. 正常路径与猜测 ID、跨项目、跨用户、跨组织拒绝；
2. 页面刷新、连接重建、Server/Profile Host/Runner 重启；
3. 并发请求、重复请求、乱序事件和过期 lease；
4. symlink、路径规范化、Workspace 删除和 Git 操作 containment；
5. Secret 不进入浏览器 DTO、日志、Artifact、配置响应和测试输出；
6. Approval 重放、并发决策、旧进程 request ID 和不确定投递；
7. Artifact 类型、Schema、授权、历史恢复和 renderer 安全；
8. Capability 未声明、不兼容和 Runtime Role 不可发现的失败关闭；
9. 真实 PostgreSQL、真实 Codex app-server 和适用的真实浏览器纵向验证。

具体命令和当前通过情况不写入本安全模型；它们分别属于组件开发规则、开发计划和
能力基线。

## 当前发布边界

当前单用户入口、单配置 Profile Host、Bearer Session、未完成的 fresh-schema
PostgreSQL 复验、Run/Thread 作用域 Artifact，以及尚未验证的执行授权上下文都不是
生产安全完成态。最新边界以 [能力基线](capability-baseline.md) 为准，修复顺序以
[开发计划](development-plan.md) 和 [路线图](roadmap.md) 为准。

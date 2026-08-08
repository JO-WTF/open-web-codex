# open-web-codex 产品与工程路线图

| 字段 | 内容 |
| --- | --- |
| 状态 | 当前接受的阶段顺序 |
| 更新时间 | 2026-08-08 |
| 时间表达 | 以能力门和结果为阶段，不承诺未经评估的日期 |
| 产品方向 | [产品愿景](product-vision.md) |
| 目标架构 | [Copilot 开发平台架构](supervisor-agent-skill-tool-architecture.md) |
| 当前能力 | [能力基线](capability-baseline.md) |
| 近期任务 | [开发计划](development-plan.md) |
| 详细实施 | [Copilot 开发平台实施计划](agent-capability-lifecycle-plan.md) |

路线图只维护阶段顺序、结果和退出条件。逐文件、类和方法级任务由详细实施计划维护；
实现事实由能力基线维护。

## 顺序总览

```text
Gate 0 可重复开发基线
  -> M2 单 Profile Copilot 创作平台
  -> M3 受治理的多用户执行
  -> M4 企业 Catalog 治理与规模化复用
  -> M5 生产 GA
  -> 仅按证据决定 Task Knowledge Ledger / Agent Decision OS
```

| 阶段 | 核心结果 | 当前位置 |
| --- | --- | --- |
| Gate 0 | 数据库、Workspace、Runtime 和当前文档重新指向同一事实 | 当前阻塞门禁 |
| M2 | 用户可从 Web/SDK 创建并运行 Tool + Skill + Agent + Supervisor Copilot | 已有仓网纵向实现，平台公共化待完成 |
| M3 | 两用户、多 Profile、授权和隔离成立 | 后续 |
| M4 | 组织级 Catalog、评价、共享、治理和规模化运维成立 | 后续 |
| M5 | 安全、恢复、容量、升级和可用性达到生产门禁 | 后续 |
| 条件阶段 | 长期知识和持续决策能力 | 未排期 |

## Gate 0：恢复可重复开发基线

### 目标

修复已应用 migration 被改写造成的开发数据库不可启动问题，使当前 schema、Provider、
Profile、Workspace、Runtime 和文档可以被同一套命令重建和验证。

### 核心交付

- 使用匹配 PostgreSQL 18 的客户端安全保留 Provider/Secret 关联并显式重建开发库；
- 空白数据库完成所有当前 migrations，连续两次启动 checksum 稳定；
- 已应用 migration 不再修改，CI 校验 migration 内容完整性；
- 当前文档删除 2.x/5.x、`planning-dataset.v2` 和“Studio 后置”的冲突；
- 单 Profile Runtime、Provider、MCP、Workspace 和浏览器健康链路恢复；
- 当前所有局部测试证据在重建后的数据库边界复核。

### 退出条件

开发环境可以从当前仓库和保留的 Provider 配置完整恢复，服务连续重启成功，能力基线
明确区分已验证与未验证能力。启动路径没有自动修复、旧 schema 猜测或隐式 Mock。

## M2：单 Profile Copilot 创作平台

### 目标

在当前单用户、单 Profile 阶段，让算法工程师能用 SDK 和 Web 创建、发布、安装、组合
并真实运行 Tool、中文 Skill、Domain Agent、Supervisor 和完整 Copilot。仓网案例是
第一条参考实现，不是平台硬编码。

### 核心交付

| 能力 | 阶段结果 |
| --- | --- |
| 公共合同 | Tool/Skill/Agent/Supervisor/Copilot 共享 Draft、Release、依赖和安装语义 |
| Work State | 通用 revision、operation、dependency、readiness、blocking input、deliverable 元数据 |
| Root Coordination | Root 只读查询 Agent execution、用户输入、Work State 和 Artifact，不依赖子 Agent 自述 |
| Data Intake | SourceAsset、mapping、Dataset Release 只有 Platform 一个 owner；领域只做业务转换 |
| Tool SDK | 受限 Python MCP SDK、contract test、打包、发布、安装、reload 和 Runtime discovery |
| Skill Studio | 中文 Skill 创建、验证、Tool capability 绑定、发布、安装和 Runtime 可见性 |
| Agent Studio | Skills、Tools、数据权限、assignment 和 deliverable 合同的组合、测试和发布 |
| Supervisor Studio | 动态协作、精确 Agent Releases、停止/部分失败规则和最终交付 |
| Copilot Builder | 自动版本/hash/lock，事务式安装，Catalog/Installation/Discovery 状态分离 |
| 执行体验 | 多 Agent 输入队列、单一 execution 卡片、wait 聚合、终态、刷新和重启恢复 |
| 上下文观测 | 每次 Provider 调用的输入/缓存/输出/延迟/compaction 与稳定前缀 fingerprint |
| 参考案例 | 完整印尼仓网 + 一个非供应链案例，不修改平台领域分支 |

### 当前阶段允许和不允许

允许：

- 当前单用户入口中的完整 Copilot 创作与运行；
- 所有数据库、缓存、事件和进程键预留 organization/user/profile scope；
- 真实业务所需的受限 Python Tool package；
- 使用现有 Runtime exact role seam，前提是普通 Thread 和受治理 Thread 隔离成立。

不允许：

- 当前就建设成员、邀请、租户管理和跨组织共享 UI；
- 任意 shell、任意在线依赖构建或无约束代码执行；
- Platform 自建 Agent scheduler、context manager、Skill interpreter 或 MCP Runtime；
- 在 Platform 识别仓网 Tool/Artifact/Agent 名称；
- Catalog 发布后未经过安装和 Runtime discovery 就宣称 ready；
- 无证据扩大 `codex-rs` 差异。

### 退出条件

1. 新用户不修改代码常量、migration、`.mcp.json` 或 Profile 隐藏配置，即可创建并运行
   一个 Copilot；
2. 所有五类资源使用同一个 Catalog/Release/Installation 生命周期；
3. 印尼仓网真实 E2E 覆盖数据、输入、两个 Agent、基线、场景、选址、地图和恢复；
4. 第二案例只新增领域包、Skills、Agent/Supervisor 和 renderer；
5. 大型数据不进入上下文，长模型调用和缓存命中可以由指标解释；
6. Runtime、刷新、重启、失败、取消、超时和乱序收敛到同一事实；
7. 当前能力基线有真实 Runtime/Web 证据。

类和方法级顺序以 [实施计划](agent-capability-lifecycle-plan.md) W0-W12 为准。

## M3：受治理的多用户执行

### 目标

在不改变 M2 能力所有权的前提下，开放多用户、多 Profile 和组织授权。

### 核心交付

- 正式登录、HttpOnly Cookie、CSRF、Session 轮换、吊销和限速；
- User/Organization/Profile/Workspace/Catalog/Installation/Data/Work State/Artifact 全链路授权；
- 每用户独立持久 Profile 和 app-server process；
- 两用户并发、猜测 UUID、缓存污染、重启和故障注入隔离矩阵；
- rootless Runner、出网、文件系统、CPU/内存/时间和 Tool 配额；
- 组织管理员、发布者、运行者和审查者的最小 RBAC；
- 企业 Tool 不可伪造的执行授权上下文；
- Artifact 和 Dataset 的跨任务授权复用；
- 显式 Push、保护分支和审计。

### 退出条件

两个组织和两个用户无法互相读取、安装、调用或控制 Profile、Workspace、Secret、
Catalog、Work State、Execution、Approval、Dataset 和 Artifact；所有拒绝发生在 owner
service，而不是靠 UI 隐藏。

## M4：企业 Catalog 治理与规模化复用

### 目标

让已经通过 M2/M3 真实验证的 Copilot 能力在组织内持续发布、评价、升级和复用。

### 核心交付

- Catalog 可见性、审批发布、弃用、维护者和来源 provenance；
- Tool/Skill/Agent/Supervisor/Copilot 的兼容性和升级影响分析；
- 安装 canary、回滚、批量 reconcile 和 Runtime readiness 监控；
- Agent/Copilot evaluation suite、黄金任务、成本/延迟/正确性基线；
- 有界 Agent 候选查询和能力检索，不向 Root 暴露整张 Catalog；
- Plugin、MCP OAuth、Memory 和 renderer 的成熟 Studio 管理能力；
- 企业模板、教程和第二方包分发；
- 使用、错误、成本和依赖健康度报告。

### 退出条件

能力发布、授权、安装、Runtime 可用性、评价和运行结果可以分别审计和失败；升级不需要
修改已有任务，回滚不依赖旧协议双读。

## M5：生产 GA

### 目标

达到可持续运行、恢复、升级和审计的生产条件。

### 核心交付

- PostgreSQL、Profile Home、Workspace、Dataset 和 Artifact 的备份恢复演练；
- HTTPS、Secret Manager、密钥轮换、安全评审和 SBOM；
- 日志、事件、Artifact、数据和审计的保留/删除策略；
- 容量、队列、Provider、Runtime、Tool Runner、磁盘和成本告警；
- rolling upgrade、Capability canary、兼容判断和回滚；
- 完整浏览器、可访问性、目标视口和故障恢复 E2E；
- 由非作者执行的部署、升级、恢复和事故手册验证。

### 退出条件

[产品设计](product-design.md) 的 GA、安全、容量和恢复要求全部有生产形态证据，不以
Fake Runtime、单进程 happy path 或文档声明代替。

## 条件阶段：长期知识与持续决策

Task Knowledge Ledger 只有在生产数据反复证明以下问题时立项：

- Artifact 和 Work State 无法表达必要的细粒度事实、假设、冲突和替代关系；
- Supervisor 持续重复整理相同证据；
- 跨任务决策无法判断依据是否过期；
- 有界摘要不足以支撑业务复核。

Agent Decision OS 还要求身份、权限、恢复、评价、Artifact 和 Ledger 全部稳定，并有
明确业务责任人、重评触发和停止规则。它不保存无限思维链，也不自动替代企业决策。

## 路线图变更规则

只有以下证据允许调整阶段顺序：

- 前置能力无法消除后续阶段的关键风险；
- 真实用户闭环证明某项能力必须前置；
- Codex 官方合同变化显著改变实现成本或所有权；
- 安全、恢复或合规门禁要求前置。

变更时同步复审产品愿景、产品设计、目标架构、开发计划和能力基线。完成历史保留在
Git；当前事实进入能力基线；长期决定进入 ADR。

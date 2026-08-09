# open-web-codex 产品与工程路线图

| 字段 | 内容 |
| --- | --- |
| 状态 | 当前接受的阶段顺序 |
| 更新时间 | 2026-08-09 |
| 时间表达 | 以能力门和结果为阶段，不承诺未经评估的日期 |
| 产品方向 | [产品愿景](product-vision.md) |
| 当前能力 | [能力基线](capability-baseline.md) |
| 当前执行 | [开发计划](development-plan.md) |
| 当前决定 | [ADR-018](adr/018-built-in-network-copilot-runtime-closure.md) |

路线图只维护阶段顺序、结果和退出条件。阶段一的逐项任务以开发计划为唯一来源；实现
事实由能力基线维护。旧 Copilot Platform/Clean Spine 计划是阶段二研究输入，不得反向
扩大阶段一。

## 顺序总览

```text
Gate 0 可重复开发基线
  -> 阶段一 内置仓网 Copilot 完整运行闭环
  -> 阶段二 公开 SDK 与 Web 创作体验
  -> 阶段三 受治理的多用户执行
  -> 阶段四 企业治理与生产 GA
```

| 阶段 | 核心结果 | 当前位置 |
| --- | --- | --- |
| Gate 0 | 数据库、Profile、Workspace、Runtime 与 evidence 指向同一事实 | 部分成立，仍需随阶段一全量复验 |
| 阶段一 | Codex 原生协作与混合数据边界驱动的完整仓网 Copilot | 实施中；现有 7/7 只是冻结原型 E3 |
| 阶段二 | 算法工程师可通过 SDK/Web 创建和组合能力 | 后续；合同需基于阶段一事实重新裁决 |
| 阶段三 | 两用户、多 Profile、授权和隔离成立 | 后续 |
| 阶段四 | Catalog 治理、容量、恢复、安全和运维达到生产门 | 后续 |

## Gate 0：可重复、无宿主继承的开发基线

### 核心结果

- 空数据库由当前 migrations 完整初始化，不猜测或修补旧 schema；
- 空 Profile 不继承服务器操作者的 auth、Skills、Plugins、MCP、Memory 或 cache；
- Provider Secret 只注入目标 Profile，实际 Runtime build 与源码 commit 可核对；
- Workspace、Profile、Runtime discovery inventory 和 evidence 可以由同一环境重建；
- 当前文档只记录现行事实、接受决定和当前计划。

### 退出条件

开发环境从当前仓库和显式平台配置完整恢复，连续重启收敛；不存在隐式 Mock、旧二进制、
宿主认证或 DB success 冒充 Runtime ready。

## 阶段一：内置仓网 Copilot 架构纠偏与完整闭环

### 用户结果

用户在一个 Workspace 中放置普通文件，创建多个独立 Task 完成不同仓网方案。Task 没有
相互通信或数据 API；同 Workspace 的 Task 可显式复用普通文件和同一授权 provider 的 exact
MCP Resource ref。用户只与一个
Copilot 对话，可以看到 Data/Network Agent 进度、回答业务问题并取得地图和报告。

### 核心交付

| 能力 | 阶段结果 |
| --- | --- |
| Workspace | Task 固定授权 Workspace；root/child 使用相同原生 `cwd` |
| 文件 | 通用列举、上传、读取、下载、删除；允许经校验相对路径 |
| 数据交换 | Workspace 文件用于用户可见/跨 package 交接；MCP Resource 用于 provider-owned typed intermediate；无 Task→Task context/result/data API |
| Runtime | 原生 spawn/follow-up/wait/mailbox/steer/child 终态与 history |
| 用户输入 | child MCP elicitation 直接桥到浏览器并返回原请求 |
| 能力供给 | Profile 托管内置 Roles/Skills/MCP；原生 discovery/reload/status |
| 仓网能力 | 数据准备、距离/成本、覆盖/SLA、成本、模拟、p-median、地图、报告 |
| 局部复用 | 路线/成本由 domain provider 按 exact pair/lane fact 复用，只补算缺失项 |
| Artifact | 只由明确 final Tool Item 注册地图、报告等用户交付，不作为数据交换 |
| 投影 | 安全、幂等、可重放，不调度 Agent 或发送 continuation |

### 阶段一禁止项

- Platform 仓网对象或数据语义；
- Data Intake、SourceAsset、DatasetRelease、DomainResource、Resource Broker；
- Workspace data revision/head/registry/binding/fingerprint/cache；
- Task 文件绑定、Task 私有文件层、COW/overlay/snapshot/worktree；
- Work State、Blackboard、Task 消息、第二调度器、Run Completion Controller；
- 绝对路径、路径逃逸、含义不明的 `source_ref` alias 和历史 asset/Dataset ID；
- Artifact 作为 Agent 输入或 Task 数据通道；
- 正式 Catalog/Release/Installation、公开 SDK/Studio/Marketplace；
- 为阶段一新增未经 Patch Map 证明的 Codex seam。

### 退出条件

1. 空 DB/Profile/Workspace 下，真实 Runtime 与内置能力 discovery 成功；
2. 用户明确选择印尼完整示例后，零 elicitation 完成全部仓网链；
3. 用户通过通用 Workspace 文件面板上传 Excel/CSV/JSON，完成全部真实交互链；
4. 同 Workspace 的 10 仓/5 仓 Task 可复用普通文件和授权 exact MCP Resource ref，
   但没有直连 context/result/data API；
5. child form、steer、mailbox、失败、取消、刷新、Profile restart 和 hot reload 通过；
6. 旧 Work State、Data Intake、DomainResource/Broker、Case/NetworkSnapshot、generic
   ResourceLink→Artifact、continuation 和假安装路径已从代码、
   schema、测试与当前文档删除；
7. Platform 没有仓网分支，Artifact 只承载交付。

## 阶段二：公开 SDK 与 Web 创作体验

### 目标

让算法工程师在不修改平台代码的情况下，通过 SDK 和 Web 创建、测试、组合并激活 Tool、
Skill、Domain Agent、Supervisor 和 Copilot。阶段二只在阶段一稳定后重新裁决具体对象与
生命周期，不自动继承 ADR-017、五对象发布链、Work State 或 typed resource 方案。

### 保持不变的边界

- Codex Runtime 继续拥有 Thread/Turn/context、Agent、Skills、Plugins、MCP 和 Tool 执行；
- Profile Host 通过官方 discovery/reload 激活能力，不把能力安装到 Workspace；
- Platform 不理解领域数据，不建设数据 registry、Dataset 或 Task 文件绑定；
- Workspace 普通文件和授权 MCP Resource 继续按各自 owner 支持显式跨 Task 复用；
- Artifact 仍只用于用户交付；
- Studio 不得成为第二个 Runtime、调度器或隐藏配置编辑器。

### 进入条件

- 阶段一双硬门全部通过；
- 内置能力包的 Profile 激活与热刷新已经有真实证据；
- SDK/Studio 所需的新持久对象有当前用户需求、owner、合同和可执行验收，而非沿用旧表。

### 退出条件

算法工程师可从规范化 Tool 开始，用中文 Skill 定义方法，以 Web 组合 Agent/Supervisor，
并让真实 Runtime 发现和运行；整个旅程不要求用户编辑 Runtime Role ID、MCP JSON、宿主
路径、hash 或内部安装状态。

## 阶段三：受治理的多用户执行

### 目标

在不改变 Runtime、Workspace 文件与 MCP Resource owner 边界的前提下，开放多用户、多 Profile 和组织授权。

### 核心交付

- 正式登录、Session、CSRF、轮换、吊销和限速；
- User/Organization/Profile/Workspace/Task/Run/Approval/Artifact 全链授权；
- 每用户独立持久 Profile 和 app-server 进程；
- 两用户、两组织、猜测 ID、缓存污染、路径逃逸和重启隔离矩阵；
- Workspace 共享成员权限与普通文件并发冲突策略；
- 企业 Tool 凭据和外部副作用的明确授权；
- Git Push、保护分支和审计。

### 退出条件

两个组织和两个用户无法互相读取或控制 Profile、Workspace、Task、Run、Approval、Artifact
与 Secret；同 Workspace 成员只按 Workspace grant 共享普通文件，不产生隐式 Task 权限。

## 阶段四：企业治理与生产 GA

### 核心交付

- 能力来源、评审、弃用、评价和可观测治理；
- Profile/Runtime canary、回滚和上游同步；
- PostgreSQL、Profile Home、Workspace 和 Artifact 备份恢复演练；
- HTTPS、Secret Manager、密钥轮换、安全评审和 SBOM；
- 日志、事件、Artifact、Workspace 和审计的保留/删除策略；
- 容量、队列、Provider、Runtime、Tool、磁盘和成本告警；
- 完整浏览器、可访问性、故障恢复和升级 E2E。

### 退出条件

能力治理、授权、Runtime 可用性和运行结果可以分别审计和失败；生产恢复不依赖旧协议
双读、隐式 fallback、Task 文件副本或 Platform 数据语义。

## 路线图变更规则

只有以下证据允许调整阶段顺序或新增平台对象：

- 当前真实用户闭环无法由现有 owner 和 Codex 原生能力完成；
- 安全、恢复或合规门禁要求新增确定性边界；
- Codex 官方合同变化显著改变实现成本或所有权；
- 可执行失败证明 Codex 原生协作、MCP Resource、普通 Workspace 文件和最终 Artifact
  的分工仍不足。

新增 registry、数据生命周期、Task 数据接口、缓存、调度器或 Codex seam 必须单独形成
ADR、owner、退出条件和真实验收，不能以“未来可能需要”为理由进入当前阶段。

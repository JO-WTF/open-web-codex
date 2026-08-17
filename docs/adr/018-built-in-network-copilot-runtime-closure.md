# ADR-018：Codex 原生机制驱动的内置仓网 Copilot

状态：已接受（2026-08-08）

替代：[ADR-017](017-clean-copilot-platform-spine.md) 的当前阶段主线，并替代 ADR-007、
ADR-009、ADR-010、ADR-013、ADR-015、ADR-016 中与本阶段 Data Intake、Dataset、
Work State、typed resource 和 Platform 协调有关的决定。被替代 ADR 只保存历史解释，
不得继续指导阶段一实现。

## 适用范围

当前阶段固定为**阶段一：内置仓网 Copilot 架构纠偏与完整运行闭环**。阶段一证明一个
内置 Supervisor、Data Agent、Network Agent、Skills 和 MCP tools 能在真实 Codex Runtime
与浏览器中完成完整仓网规划；不证明公开 SDK、Studio、Catalog、Marketplace、第二领域
或多用户产品入口已经完成。

当前执行顺序和验收门只以 [开发计划](../development-plan.md) 为准。当前实现事实与能力
证据分别以 [系统架构](../architecture.md) 和 [能力基线](../capability-baseline.md) 为准。

## 背景

现有 7/7 仓网 E2E 证明预接好的 Root、Data Agent 和 Network Agent 可以在真实 Runtime
中运行，但脚本仍直接创建 Work State、复制定义，并向 Prompt 注入内部 ID、字段形状、
工具顺序和禁止事项。源码还存在 Workspace capability 扫描、false installation、Platform
continuation/global interrupt、Data Intake、SourceAsset/Dataset、Case SQLite 和 wire aliases。

Codex Runtime 已经拥有 Thread/Turn/context、spawn/fork/follow-up、mailbox/wait、steer、
child 终态、Skill/Plugin/MCP discovery 与刷新，以及 MCP elicitation。继续增加 Platform
调度、输入中继或可信资源协议只会形成第二 owner。

## 决定

### 1. 按数据生命周期复用 Codex 原生交换面

Task 在创建时固定一个授权 Workspace。Root 与所有 child Thread 使用 Codex 官方 `cwd`
和原生 sandbox metadata 访问该 Workspace。同一 Workspace 下的 Task 可以按用户意图复用
普通文件；不同 Workspace 必须隔离。

阶段一不建设 Platform 自有的 Task-to-Task 消息、上下文、结果、数据或 adopt API。数据交换按
官方 owner 分为四类：

- Root/child 的在线协作使用 Codex Thread context、`fork_turns`、mailbox、follow-up 和 steer；
- 大型或类型化中间数据使用 MCP provider 拥有的 Resource，Codex 负责 Tool Item、ResourceLink
  和 `read_resource` 调用语义；
- 用户上传、可见可编辑数据、跨独立 MCP package 的显式交接与导出使用普通 Workspace
  文件；
- 地图、最终报告等明确用户交付使用 Artifact，不回流为 Agent 输入。

MCP Resource 的内容和持久化由 provider 拥有，Codex 不自动把它复制到另一 Thread。同一
Profile 中的其他 Thread 只有在配置了同一命名 provider、获得授权且知道精确 URI 时才可读取。
Web Server 可以从官方 Thread history 的精确 Item ID 构建可重建的授权/可发现引用投影，
但不得存储 Resource 内容、建立通用 Resource Broker 或形成第二数据系统。

Resource 的目的只是复用一个确切、类型化的已处理或已计算快照，避免重复文件读取、字段映射、
归一化和 pair 计算；它不是 Workspace 文件访问禁令，也不是来源真实性、业务可信度、版本治理或
自动复用许可。来源确认、数据质量报告和业务判断仍各自负责这些结论。

阶段一不建设：

- COW、overlay、snapshot、Task-owned worktree 或 Task 私有文件层；
- WorkspaceDataObject、revision、head、Schema registry 或 read binding；
- TaskSourceAsset、DatasetRelease、DomainResource 或通用 Resource Broker；
- Platform 文件/数据语义 registry、fingerprint 复用门或计算 cache/binding。

SHA 可用于字节完整性、ETag 或 provider 物理去重，不能代替授权或用整份数据集指纹
决定是否复用。路线和成本的局部复用由领域 Tool 以 pair fact 为粒度判断，不属于 Platform
可信协议。

### 2. 文件路径和 Resource 引用各用官方定位合同

允许用户、Skill、模型和 Tool 使用经校验的 Workspace 相对路径。Platform 和 Tool 必须
拒绝服务器绝对路径、路径逃逸、symlink 逃逸、含义不明的 `source_ref` alias 与历史
asset/Dataset ID。MCP Resource 使用其所属 provider 和官方 URI，不伪装成 Workspace 路径。
浏览器不得看到服务器绝对路径或未授权的 provider 内部位置。

Platform 只负责通用文件列举、上传、读取、下载、删除和路径边界。保存哪些文件、读取
哪些文件、如何映射、裁剪、合并、补算或覆盖，由用户要求、Skill 与 Tool 决定。
Platform 不理解仓网字段、数据版本、候选仓、路线、矩阵或方案语义。

### 3. 最大化复用 Codex 原生能力

| Owner | 阶段一职责 | 明确不负责 |
| --- | --- | --- |
| Codex Runtime | Thread/Turn/context、cwd、spawn/follow-up、wait/mailbox、steer、child 终态、Role、Skill/Plugin/MCP discovery/reload、Tool execution、MCP elicitation | Platform Task/Run、浏览器 DTO、Workspace 授权和 Artifact |
| Platform | Task→Workspace 映射、授权与多用户 scope、Profile 进程、通用 Workspace 文件 Web API、Approval/最终 Artifact、Runtime 事件安全投影与有界 Resource 引用可发现投影 | Resource 内容、数据语义、Agent 调度、业务状态机、continuation 或第二 mailbox |
| Skill | Agent 职责、业务顺序、文件选择与命名、复用策略、问题和交付标准 | 授权、路径逃逸检查、Runtime 生命周期 |
| MCP Tool/provider | 文件格式/字段校验、Resource 内容与读取模板、原子读写、业务算法、pair-level 确定性复用、外部副作用确认、地图和报告生成 | Platform 身份、Task 间上下文或隐藏业务状态机 |

Supervisor 必须根据 child 是否需要 Root 历史显式选择 `fork_turns`，不由 Platform 复制或摘编
上下文。Root 使用原生 wait/mailbox/steer，并可用 follow-up 复用同一 child。Event Projection
只投影，不发 Turn、不调度、不 interrupt。Resource 引用通过官方 Tool Item/消息或用户显式
选择传递，不由 Platform 偷渡子 Agent 上下文。

child 发起的 MCP form 由原 child Turn 拥有，Platform 只转成安全浏览器卡片并把
Accept/Decline/Cancel 返回原请求。Root 不复制或中继阻塞工具的问题。

### 4. 内置能力通过 Profile 原生发现与热刷新

阶段一把 Supervisor、Data/Network Roles、Skills 和 MCP activation 作为一组 Profile-owned
内置能力托管，但不把它包装成 Codex Plugin 或 Platform Release。Supervisor 是 Root 可发现
的 Profile Skill；Data/Network 是 Codex 原生 Agent Role，并各自通过 Role config 启用所需
Skill、MCP server 和 tool allowlist。Skills 使用 `$CODEX_HOME/skills`、Roles 使用
`$CODEX_HOME/agents/*.toml` 自动发现，完整 MCP transport 与 `enabled_tools` 直接位于各自
Role TOML。阶段一不改 Profile `config.toml`、不收窄全局 Role allowlist，也不覆盖 Codex
内置或用户自建 Role。checked-in 文件只是 clean Profile 默认 seed；创建后 Profile 成为运行态
owner，平台启动不覆盖其内容。当前仓网验收 composition 仅在 owned `app-server` 进程启动前，
通过 Codex 官方 feature override 关闭 `plugins`、`remote_plugin`、`apps` 和
`tool_suggest`，以缩小该 Profile 的模型可见面；这不是 Platform 自建 discovery/filter，且不
修改持久 Profile 文件。显式 reset 或升级迁移不在阶段一。

仓网 Python/地图 launcher、代码与只读 Mock fixture 属于显式部署的只读 application assets，
不复制到每个 Profile，也不从 process cwd、Workspace 或源码树扫描发现。Profile 配置只引用
已校验的应用资产根；缺少配置或预期 launcher 时明确 unavailable。stdio MCP config 不设置
cwd；provider Resource scope 复用官方 Thread Workspace cwd，业务文件也只能通过当前 Turn
的 native `sandboxCwd` 定位。maps credential/resource 状态根属于 Profile 私有 runtime，
不得写入共享 maps asset root。Mock 只有经用户明确选择后，才能由 Tool 从只读 application
assets 复制为 Workspace 普通文件。

热刷新只承诺 Codex 已有的真实语义：Skill watcher/`forceReload` 使下一 Turn 使用新 Skill；
Role 文件在每次 `spawn_agent` 时重读，使下一次 spawn 使用新内容；
`config/mcpServer/reload` 在安全 step 边界原子发布新 MCP runtime，活动 step 继续持有旧
runtime。Role 集合、allowlist、删除或改名只保证在新 Root Thread 生效，不为既有 Thread
补新的 Codex seam。

Readiness 只能来自当前 Profile Runtime 的 `skills/list`、`config/read`、config warning、
MCP startup status 和 `mcpServerStatus/list`。Codex 当前没有正向 Role inventory，因此 Role
只能报告 `configured` 或 `unavailable`；真实可执行性由 native spawn 验收。reload 或查询失败
立即 unavailable，不沿用旧 ready，也不持久化另一份 installed/readiness 状态。

阶段一不建设正式 Catalog/Release/Installation，也不使用 Plugin Marketplace 或
`selectedCapabilityRoots` 承担 built-in 热加载。后者的 executor discovery 是 Thread-scoped
且没有 invalidation，成功和失败都会缓存，不符合本阶段热刷新合同。不得把能力写入
Workspace，不得扫描 process cwd、Workspace 或源码仓库发现能力，不得以数据库
`installed` 代替 Runtime 观察结果。

### 5. 仓网业务只存在于 Skill 和 Tool

Data Agent 从用户指定的 Workspace Excel/CSV/JSON 读取数据，完成画像、字段映射、
标准化、行政区和候选仓补全。首次 normalized input 保留全部确认的候选仓；只有 candidate
新增、替换或移除可以通过 `candidate_warehouse_delta.v1` 加 exact base ref 派生一个新的完整输入
快照。需求、现网仓、实际分配、路线或报价事实变化必须完整归一化。用户可见数据和跨 Workspace
交接写 Workspace 文件；provider-owned typed intermediate 可发布为 MCP Resource。Data 与 Network
是同一个 `supply_chain` provider Resource 域的不同 Tool，Network 通过 package-owned typed facade
读取 exact normalized ref，绝不扫描或重解析原始文件。Network Agent 完成距离与时长、成本、覆盖、
SLA、全网成本、增减搬迁模拟、p-median、服务约束规划、地图和报告。路线/成本 Tool 以 pair fact
粒度复用已有真实计算，只对缺失 pair 重算；不以整份数据版本指纹作为复用门。

Network 从任意 exact baseline、scenario 或 location assignment 结果产生语义 GeoJSON：城市、设施、
实际分配和直线覆盖关系都来自坐标与分配事实，不含城市、仓库或 XD 名称分支。Maps 只消费 exact
GeoJSON ref，保存 immutable `map_card_spec.v1`（几何引用、图层、视角和可选 parent spec ref）。纯样式
调整产生 child spec 并复用原 GeoJSON；新增覆盖线先由 Network 发布新几何，再由 Maps 创建 child spec。
Platform 只投影 card ref/spec ref，不保存 GeoJSON 内容。

Platform API、数据库、DTO 和 projection 不出现仓网对象或仓网状态机。Tool 默认
create-new；同名时通过原生 elicitation 询问覆盖、改名或取消。Mock 只有在用户明确选择
“印尼仓网完整示例”时才写入可见普通文件，真实输入失败不得静默回退。

### 6. Artifact 只承载显式用户交付

Artifact 只用于地图、最终报告和明确交付件的授权展示、下载与保留。结构化中间数据继续
由 Workspace 普通文件或 MCP Resource 拥有。Artifact 不作为 Agent 输入、Task 间数据通道或工作流门禁；
Runtime/Tool/Artifact 状态不能由模型文本冒充。

只有已明确列入交付合同的 final report/map Tool，才能从其精确官方 MCP Tool Item 的
structured result 注册 Artifact。Platform 不得把任意 ResourceLink 自动提升为 Artifact，不得通过
模型文本指令、工具名猜测或跨 child Resource 搜索识别交付。物化字节的 content SHA 只是完整性
证据，不是业务复用门。

### 7. 旧项目路径不兼容

每个 owner 切换时必须在同一 change 更新调用方、schema、fixture、UI、测试和当前文档，
并删除旧实现。不得保留 aliases、双读、双写、fallback 或隐藏旧 E2E。

阶段一至少删除：

- Work State service/routes/MCP/gates 与 Platform coordination MCP；
- Data Intake、SourceAsset、DatasetRelease、DomainResource、Resource Broker；
- 供应链 CaseRepository、NetworkSnapshot、`workspace_dataset`/Indonesia release server、Profile SQLite
  黑板和 `source_ref` aliases；
- ResourceStore 保留为 MCP provider 的最小内容 owner，删除叠加的 ArtifactRef、taskEvidence SHA、
  整数据集复用门与双重 ref；
- Event Projection continuation Prompt、主动发 Turn和 global interrupt；
- generic ResourceLink→Artifact、跨 child Resource lookup 与 Artifact 数据回流；
- Supervisor policy snapshot/binding、Platform continuation，以及 provider speculative hashes 和
  历史 SHA password 兼容；
- false Catalog/Installation、capability path scan、DB-only readiness；
- 通过 Prompt 注入内部 ID 和手建 Work State 的旧 E2E。

## 实施状态（2026-08-09）

ADR 的上述决定不变。Slice 4B.2-A 已按第 7 节完成其中直接属于 Platform 第二控制面与
历史可信链的删除：Work State/coordination crate、route、MCP、gate 与十张 schema 表，
Supervisor snapshot/binding/definition/revision/release/continuation 六表及其只读浏览器表面，
Event Projection 的主动 continuation/Root Turn，provider speculative SHA 四列，以及历史
SHA password compatibility 均已不存在。当前协作事实只由 Codex 原生 Thread/Turn/Agent
lifecycle 保存；Platform 只分别保存其可重建 Runtime projection、Approval 与 final Artifact。
完整仓网业务闭环和
第 7 节其余领域 Tool/Artifact 清理仍未完成。fresh PostgreSQL security、native child
projection、Workspace reuse 和受影响 Rust/Web 定向门是本次删除的验证证据。

## 阶段一验收

阶段一完成必须同时满足：

1. 空 DB、空 Profile、干净 Workspace 下，真实 Runtime/Provider 与内置能力 discovery 成功；
2. 用户明确选择印尼完整示例后，以官方 Thread/Agent/MCP Resource 与必要的
   Workspace 文件完成零 elicitation 全链；
3. 用户通过通用 Workspace 文件面板上传真实 Excel/CSV/JSON，完成全部业务交互链；
4. 同 Workspace 的 10 仓/5 仓 Task 可复用普通文件，同一授权 provider 可按精确
   Resource ref 复用中间数据，但没有 Platform Task 上下文/结果直连 API；
5. child form、steer、mailbox、失败、取消、刷新、Profile restart 和 hot reload 有真实门；
6. 路线/成本部分变更只重算缺失 pair；Artifact 只包含显式最终交付，Platform 没有仓网
   状态机，旧重型路径已从代码和当前文档删除；
7. 没有新增未登记 Codex seam，也没有第二调度器、durable mailbox 或 exactly-once。

现有 7/7 脚本只能作为冻结原型回归证据，不能计入完成。

## 后果

- Data Agent 的长耗时准备可按用户与 Skill 的要求写用户可见 Workspace 文件，或保存为
  provider-owned MCP Resource 供后续授权 Task 按精确 ref 复用；Platform 不裁决业务等价或最新版。
- 共享 Workspace 的并发写冲突由通用路径边界、Tool 原子 create-new 和显式覆盖确认处理，
  不通过 Task 私有副本隐藏。
- 公开 SDK、Studio、Catalog、Marketplace 和多用户产品入口后移；未来设计也不得默认
  引入 Platform 数据语义或第二 Runtime，除非有新的证据和 ADR。
- 任何 Codex 修改都必须先有官方机制不足的可执行证据、Patch Map、测试和退出条件；
  本 ADR 不授权新增 Codex seam。

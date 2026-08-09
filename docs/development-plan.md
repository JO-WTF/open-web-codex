# open-web-codex 开发计划

| 字段 | 内容 |
| --- | --- |
| 文档性质 | 当前与下一里程碑的执行计划 |
| 更新日期 | 2026-08-10 |
| 当前阶段 | 阶段一：内置仓网 Copilot 架构纠偏与完整运行闭环 |
| 当前状态 | 实施中；现有脚本接线的 7/7 E2E 不是阶段一完成证据 |
| 当前阶段裁决 | Codex 原生协作与 MCP Resource + Workspace 文件 + 最终 Artifact 混合边界；ADR-018 已接受并作为当前基线 |
| 当前事实 | [Architecture](architecture.md) 与 [Capability Baseline](capability-baseline.md) |
| 后续阶段 | 公开 SDK、Studio、Marketplace、第二领域和多用户产品流程 |

本文只维护阶段一尚未完成的工作、顺序和退出门。用户心智统一为：

> Workspace 管理用户文件，Task 做一个独立方案，Copilot 通过原生协作和领域工具处理、复用并交付数据。

阶段一不建设平台数据操作系统。Task 之间没有直接消息、上下文、结果或数据接口；同一
Workspace 下的 Task 可按用户意图复用普通文件，也可通过同一授权 MCP provider 的精确
Resource ref 复用中间数据。保存什么、读取什么、怎样复用、裁剪、合并或覆盖，由用户
要求、Skill 和 Tool 决定，Platform 不理解仓网数据语义。

## 1. 阶段目标与边界

阶段一交付一个内置仓网 Copilot：Root、Data Agent 和 Network Agent 复用真实 Codex
Runtime，在浏览器中完成数据准备、距离与成本、覆盖、SLA、模拟、选址、地图和报告。
用户与一个 Copilot 对话；Root 在 child 运行期间仍可响应用户、接收补充要求和更新进度。

阶段一必须同时完成：

1. Task 在创建时固定一个授权 Workspace，Root 与所有 child Thread 使用该 Workspace。
2. 直接复用通用 Workspace 文件列举、上传、读取、下载、删除和原子写入能力，并复用
   Codex 官方 MCP Resource/ResourceLink/read_resource 语义。
3. Data/Network Tool 以 Codex 原生 sandbox metadata 解析 Turn Workspace，对用户文件使用
   经校验的 Workspace 相对路径，对 provider-owned intermediate 使用 typed Resource ref。
4. Supervisor、Roles、Skills 和 MCP tools 由 Profile 托管，并通过 Codex 原生发现与热刷新
   生效，不走正式发布流程。
5. child 发起的结构化问题直接通过原生 MCP elicitation 到达浏览器，答案返回原 child
   request；Platform 不中继、不续跑、不全局 interrupt。
6. 默认印尼 Mock 完整链和真实 Excel/CSV/JSON 交互链均通过 Web E2E。
7. 删除 Work State、Data Intake、SourceAsset、DatasetRelease、DomainResource、Resource
   Broker、Case store、generic Resource→Artifact、Artifact 输入传输和 Platform continuation 等旧路径。
8. Web 可从 Root Task 查看 child Agent 的官方 Thread/Turn/Item 执行过程；实时与刷新恢复必须
   一致，不得把不同的 command、MCP Tool、进度、等待、审批和终态统一压成通用占位文案。

阶段一不交付公开 SDK、Studio、五对象 Catalog/Release、Marketplace、第二领域或多用户
产品流，也不建设 Workspace 数据对象、数据 revision/head、Schema registry、Task 数据绑定、
Task 私有文件库、COW/worktree、Platform 计算缓存或整数据集语义指纹、Blackboard、第二调度器、durable
mailbox、Workflow DSL、Run Completion Controller、签名链或 exactly-once。

## 2. 唯一所有权合同

| 事实 | Owner | 阶段一规则 |
| --- | --- | --- |
| Thread、Turn、context、spawn/followup、wait/mailbox、steer、child 终态 | Codex Runtime | Platform 只桥接和投影，不重新实现 |
| Skill、MCP、Role config 发现与刷新 | Codex Runtime；Profile Host 负责托管固定内置 Skill/Role 与 MCP activation | built-in 不使用 Plugin/selected roots；观察结果必须来自当前 Runtime |
| 仓网 Tool 代码与只读 Mock 源 | 显式配置的共享应用资产 | 不复制到 Profile，不从 cwd、Workspace 或源码树扫描 |
| Task 与唯一执行 Workspace | Platform | Task 创建后固定；Run、resume、fork 不得改选其他 Workspace |
| Thread cwd 与 MCP sandbox metadata | Codex Runtime | Root/child 都继承授权 Workspace；不新增 Codex seam |
| 普通数据文件 | Workspace 文件系统 | 同 Workspace Task 天然可见；没有 Task→文件 binding 或数据生命周期 |
| typed intermediate 内容与生命周期 | MCP provider | 通过官方 Resource URI/template/read 使用；Platform 不存内容或建通用 Broker |
| Workspace authority | Platform/Runner | authenticated Workspace capability、canonical containment/no-follow、final file atomic create-new 与 Artifact 授权物化 |
| 通用 Copilot provider primitives | 长期为平台提供的 provider library；阶段一内嵌在 `supply_chain` | `ResourceRef` envelope、expected-schema validation、canonical codec、payload bounds、typed errors、provider-scoped load/publish；必须领域无关并由 Data/Network/final 共用 |
| Resource ref 可发现与授权 | Codex official Item + 有界 Platform projection | 只引用 exact itemId 与 `{server, uri}`，可重建，不猜模型文本 |
| 文件选择、字段、映射、标准化、复用和合并 | 用户 + Skill + Tool | Platform 不分类、不猜测、不限制业务复用 |
| Agent 协同方法 | Supervisor Skill | 不落 Platform workflow 状态机 |
| 仓网规划能力 | Data/Network MCP domain Tool bindings、Role/Skill 与 pure owners | 阶段一统一拥有 Data mapping/normalize/geography、route/cost facts与matrix、baseline/scenario/p-median/comparison 以及 final map/report；输入输出为 Workspace 相对路径、typed Resource ref 或普通业务参数，不拥有通用 scope/store/codec/writer。未来公共供应链领域层只记为触发式 TODO，不增加当前实现层、提交线或验收节点 |
| Agent 活动与问题卡片 | Codex Runtime 提供 child Thread/Turn/Item 事实；Platform projection 只做授权、安全裁剪和浏览器 DTO | 保留 item type、Tool 身份、bounded action/result、生命周期、错误与时间顺序并容忍乱序；不另建 child 日志、执行历史或状态机，不用通用占位文案替代可安全展示的官方事实 |
| 地图、报告和明确交付件 | Artifact | 只展示和下载，永远不作为 Task/Agent 输入或交换介质 |

### Platform 只执行的硬约束

- 用户、Organization、Profile、Workspace 的授权与 scope；当前单用户也不得退化为无 scope
  全局状态。
- Task 所属 Workspace 与 Runtime `cwd` 的一致性；浏览器不能提交服务器绝对路径。
- Workspace 文件相对路径、大小、regular-file、符号链接逃逸和跨 Workspace访问检查。
- Task/Run/Approval/Artifact 的持久化、浏览器 DTO 与安全事件投影。
- SHA 若存在，只用于字节完整性、ETag 或 provider 物理去重，不作为授权或整数据集复用许可。

### Tool 执行的硬约束

- 允许用户、Skill、模型与 Tool 使用 Workspace 相对路径与 typed MCP Resource ref；拒绝
  绝对路径、`..` 逃逸、含义不明的 `source_ref` alias 和历史 asset/Dataset ID。
- Data Tool 对 Excel/CSV/JSON 的格式、结构和业务字段负责；Platform 不保存映射状态。
- 写文件使用临时文件和同目录原子 rename；默认 create-new，不静默覆盖已有文件。
- 文件重名时通过原生 elicitation 询问覆盖、改名或取消。
- 外部导航前展示路线数量、预计调用量和费用风险，并取得用户许可。
- 路线和成本按 exact pair fact 复用已有真实计算，只计算缺失 pair，并分开报告
  reused/computed 数量。
- 中途失败不得留下看似完成的结果文件。

### Skill 负责的行为

- 读取哪些文件、何时询问用户选择文件、如何解释缺失和歧义。
- 哪些中间数据或结果写入 Workspace，哪些保存为 MCP Resource，以及对用户如何呈现。
- 如何复用已有文件或 Resource、复用多少、如何裁剪、转换、合并或补算。
- Data/Network Agent 的职责、顺序、业务问题、默认值和交付结构。

## 3. 已验证且优先复用的原生基础

- 通用 `/workspaces/{id}/files` 已提供列举、上传、文本读取、二进制下载和删除；
  `GitRuntime` 已有 Workspace 授权、相对路径、symlink escape、单文件原子写和审计。
- Codex MCP runtime 已携带 Turn Workspace；`native_workspace_probe` 已证明 Root 与 native
  child 的 Python FastMCP 调用从 `codex/sandbox-state-meta.sandboxCwd` 解析业务 Workspace，
  Plugin 进程 cwd 没有被当作业务 Workspace。
- native child completion/mailbox、Root early final、forked context、MCP elicitation、Skill
  reload/watcher、MCP runtime refresh、fork/LRU/restore、Stop 和 PreToolUse 定向测试已通过。
- `selectedCapabilityRoots` 的 executor discovery 是 Thread-scoped 且没有 invalidation，成功与
  失败都会缓存；它不能作为 built-in 热加载基础。阶段一改用 `$CODEX_HOME/skills` 和
  `$CODEX_HOME/agents/*.toml`，MCP transport 只放在 Data/Network Role TOML 中，不走
  Plugin/Marketplace/Installation 或全局 Profile MCP 配置。
- Role 文件内容由 Codex 在每次 spawn 时重读；既有 Root Thread 的 Role 集合、allowlist 和
  spawn limits 不会随 user config reload 完整重算。内置 Data/Network Role 必须在 Root Thread
  创建前注册；内容修改对下一次 spawn 生效，角色增删改名只保证对新 Root Thread 生效。
- `SubAgentActivity Started` 不保证早于 child 首个 tool event；Platform projection 必须按
  thread identity 幂等并容忍乱序，不能据此申请新 Runtime seam。

该 Probe 已使用当前 checkout 构建的 Codex app-server、真实 Profile Host、native
`collaboration.spawn_agent`、官方 `turn/steer` 和 Python FastMCP 通过；阶段一没有因此新增
Codex seam。

## 4. 实施顺序

### Slice 0：同步轻量基线并完成 native Workspace Probe

状态（2026-08-09）：已完成。权威文档已统一到 ADR-018；native Workspace Probe 通过；
没有新增 Codex seam 或未消费数据框架。

1. 以本计划重写 ADR-018、Architecture、安全模型、能力基线和相关当前文档；撤销此前
   Workspace revision、cache、binding、typed publish/adopt 与 TaskSourceAsset 结论。
2. 在 root 与 child FastMCP tool call 中断言 `sandboxCwd` 等于授权 Workspace，同时断言
   Plugin process cwd 不能成为业务数据根。
3. 固化现有 wait/mailbox/steer/elicitation/reload 证据；Patch Map 明确阶段一不新增 Codex
   seam。
4. 从目标设计删除 Dataset/DomainResource/Resource Broker、Schema registry、logical head、
   read binding、semantic fingerprint/cache、Task file store 和 ExecutionBinding 业务扩展。

退出：权威文档不再要求 Platform 数据 registry/binding/fingerprint/cache；native Workspace Probe
通过；不存在为以后保留的未消费数据框架。

### Slice 1：Task 固定执行 Workspace

状态（2026-08-09）：核心合同与执行不变量已完成并通过定向验证。Task 创建固定唯一授权
Workspace；Browser 不再为 Run 提交 Workspace；数据库、HTTP、orchestrator、fork 执行与
恢复路径均强制 Run Workspace 等于 Task Workspace。跨 Task/Workspace fork 已拒绝。
同 Workspace 多 Task 的普通文件读写、路径拒绝和文件冲突矩阵将在 Slice 2 的真实文件产品
流中一起验收，因此阶段一整体仍未完成。

1. 在当前 Task 合同中直接保存唯一 `workspace_id`，不新增 binding 表。
2. 创建 Task 时验证 Project、Profile、Workspace grant；StartRun 从 Task 解析 Workspace，
   Browser 不再为每次 Run 自由选择另一个 Workspace。
3. Run 继续记录实际 Workspace 作为执行审计，但必须等于 Task Workspace。
4. start、resume、fork、follow-up 均通过官方 app-server 合同使用同一 Workspace；child 使用
   Codex 原生继承，不依赖异步 projection 决定权限。
5. 不创建 Task-owned checkout、worktree、clone、overlay、snapshot 或 COW。

退出：同 Workspace 的两个 Task 能读取同一普通文件；Indonesia 与 Thailand Workspace 无法
经相对路径、绝对路径、`..` 或 symlink 交叉读取；Task 恢复后仍使用原 Workspace。

### Slice 2：收敛通用 Workspace 文件产品面

状态（2026-08-09）：已完成并通过定向验证。普通 Workspace 文件已经是当前用户上传、浏览与编辑的产品数据
面；旧 Platform Data Intake/SourceAsset/Dataset/Task attachment 生产路径、冻结能力包与旧
E2E 入口已删除。阶段一整体仍未完成，下一切片是 Profile 托管原生热加载。

1. 直接复用现有 `/workspaces/{id}/files` 与 `GitRuntime`，只补真实缺口，不建设新文件服务、
   文件 registry、数据页或第二套选择器。
2. Web 统一通过 Workspace 文件面板上传、搜索、查看、下载和删除；Task 对话只允许引用
   已有 Workspace 相对路径，不再上传或附加 SourceAsset。
3. 上传成功后文件立即作为普通 Workspace 文件出现；同内容不同路径仍是两个普通文件。
4. HTTP 每次只接收一个 multipart file；Web 多选逐文件处理。已有目标返回 typed conflict，
   用户对当前文件选择覆盖、改名或取消；不承诺批次原子、内容寻址、版本或输入快照。
5. 删除 Data Intake Draft/session/mapping/gate/hold、SourceAsset 表与专用文件 API、
   DatasetRelease 路由/表、Task dataset/attachment/snapshot/projection、`analysis_start`、
   `source_asset_ids` Prompt 注入和相应 UI/fixture/tests。

退出证据：fresh DB migration 断言 Platform 数据库不存在 Task→文件关系和旧 Data Intake/
Dataset 对象；页面不再出现 Dataset、Release、binding、revision、source asset 入口；同
Workspace 两个 Task 可读取同一路径文件，另一个租户的 list/content/delete 均拒绝；absolute、
`..`、symlink 与同名写冲突由 Runner/HTTP 测试覆盖；多-part request 在落盘前拒绝。

### Slice 3：Profile 托管的原生热加载

状态（2026-08-09）：已完成。3A 已审计原生能力；3B.1 已交付 clean Profile native
Skill/Role seed、显式共享应用资产 composition 与四个 MCP 的真实 stdio inventory probe；
3B.2 已删除 built-in/product Thread 的 selected-root 注入，并通过 clean Profile Runtime
discovery/native spawn/mailbox/reload/hot-boundary gate；3B.3 已把 Run admission 收敛为
Standard，删除 Governed runtime、RunStartPreflight、`platform-agents` 第二启动系统、
Catalog/Studio/Python publish 生产系统及其失去 owner 的旧 DB schema。阶段一整体仍未完成，
下一步只进入 Slice 4 的原生协同与第二调度器清理。

1. 把内置 Supervisor、Data/Network Role、Skills 和 MCP activation 作为固定的 Profile-owned
   仓网能力托管；仓网 Tool 代码与只读 Mock 源由显式配置的共享应用资产持有。不建立
   Release/Installation、registry、版本状态机或哈希信任链。Supervisor 是 Root Skill，不新增
   独立发布对象。
2. 使用 Codex 原生 `$CODEX_HOME/skills` 与 `$CODEX_HOME/agents/*.toml`。Role TOML 自身
   持有完整 MCP transport 和 `enabled_tools`，不改 Profile `config.toml`、全局 role allowlist
   或 Codex built-in roles。Profile Host 只为 clean Profile seed 固定 Skill/Role 默认文件；
   已存在普通文件归 Profile 所有并保留。Server 绑定经过校验的显式应用资产根；Runtime 不
   扫描 process cwd、Workspace 或源码树。缺少配置或预期 launcher 时明确 unavailable。
3. Root 只暴露原生多 Agent 面，不配置全局仓网 MCP；Data/Network Role 通过原生 Role
   config 启用各自 Skill、MCP server 和 `enabled_tools`。MCP 进程 cwd 是 canonical 只读应用
   Tool root；maps credential/resource state 位于 Profile 私有 runtime，业务 Workspace 只能
   来自 native `sandboxCwd`。工具代码、共享 venv、Node 依赖、Mock、测试和缓存均不复制进
   Profile。
4. Skill watcher/`forceReload` 对下一 Turn 生效；Role 文件修改对下一次 spawn 生效；MCP
   reload 在安全 step 边界切换。Role 集合、allowlist、增删改名只对新 Root Thread 保证，
   不修改 Codex 弥补这一边界。
5. Runtime 观察只使用 `skills/list(forceReload=true)`、`config/read`/config warning、
   MCP startup status 和 `mcpServerStatus/list`。Role 状态只称 `configured/unavailable`，真实
   执行能力由 Root 对 Data/Network 的 native spawn gate 证明；不保存 DB readiness。
6. built-in/product Standard/fork 路径已经删除 capability path scan 与 selected-root 主动
   注入。3B.3-B2 进一步删除 `RunStartPreflight`、Governed Agent/Supervisor mode、旧
   `platform-agents/<definition>/<version>` writer/verifier 与 request-scoped SHA Role/inventory
   校验。3B.3-B3 已删除 Catalog/Studio/Python publish crate、route、DTO、client、UI 与
   Workspace package publisher；3B.3-B4 又删除失去生产 owner 的旧数据库对象。保留 Codex 内
   已登记的 selected Plugin policy seam，但 built-in 不调用它。

3B.1 已有证据：Profile startup seed 单测 5 项、Server composition 单测 3 项均通过；真实
stdio probe 启动 Data/Demo/Network/maps 并断言 Role 所需 inventory，两个共享应用资产树
启动前后无新增文件或 mtime 变化。该 probe 不是 Runtime discovery/native spawn gate。

3B.2 已有证据：使用当前 checkout 构建的 Codex、真实 Profile Host、生产 built-in
composition 与本地 mock Responses provider，clean Profile 的官方 `skills/list` 精确发现三项
仓网 Skill，Root 不发现仓网 MCP，Data/Network native child 只看到各自 Skill 和精确
`8+1`/`14+5` MCP tool inventory。原生 wait/mailbox、同 Data child follow-up 与所有 terminal
通过；Profile Skill 修改触发 `skills/changed` 并由 force reload 读取，Profile Role 修改只影响
下一次 spawn。既有 child 的无 delta MCP reload 在 safe boundary 保持同一 inventory，不伪造
startup transition。malformed Role 发出 config warning，spawn 明确失败、不创建 child，Root
仍完成。Profile Host 的私有 HOME 与 canonical 0700 neutral process cwd 阻止宿主
`$HOME/.agents` 和 Runner/源码祖先 `.codex` 污染 clean Profile；Thread cwd 仍是授权
Workspace。该 gate 证明 built-in Runtime composition，不替代仓网业务 E2E。

3B.3-B2 已有证据：Adapter、Profile Host、Run Orchestrator 和 Server
定向测试通过；唯一启动调用是 Standard start/fork。真实 Indonesia/Thailand 探针通过 Profile
原生 Skill/Role/MCP seed 启动 Root 与 child，二者只读取各自授权 Workspace，MCP process
cwd 保持共享应用资产根，Workspace 中没有 capability 文件。clean Profile activation/hot
boundary、malformed Role 和 B1 fork/idempotency PostgreSQL 门均继续通过。

3B.3-B3 已删除 `capability-catalog`/`supervisor-catalog` crate、所有 Agent/Supervisor/
Instruction Policy definition 与 Python capability publish route、Browser authoring DTO/client、
Agent Studio/三项 Settings Studio，以及 GitRuntime 的 `.open-web-release.json` package publish/
verify 特殊语义。随后 Slice 4B.2-A 删除了 `/runs/{id}/supervisor-policy` 只读投影；
原生 Profile Agents/Skills/MCP、普通 Workspace 文件和通用 Artifact 保留。

3B.3-B4 新增 current-schema migration，按显式 FK 顺序删除 Capability Catalog
Draft/Release/Installation、Agent definition/release/run binding、Workspace capability package
release 与 Supervisor instruction policy release 共 14 张无 owner 表。fresh disposable PostgreSQL
从完整 migration 链初始化后断言这些表全部不存在。Slice 4B.2-A 的 migration 55 继续删除
Supervisor policy/definition/release/continuation 六表和 Work State 十表；fresh gate 保留
Runtime agent projection 与 provider_call_metrics，并断言 provider speculative hash 四列
不存在。Slice 4B.2-B 已由 migration 56 删除 `profile_capabilities`，并从 Codex initialize、
ProfileHost、Server 和 Web 删除本地 capability manifest；ProfileHost 只 typed 校验官方四字段
和 owned `codexHome`。测试库随后被精确删除。

退出：干净 Profile 在 Root Thread 创建前完成固定 Role 注册并发现三项 Skill；Data/Network
native spawn 成功且只看到各自允许的 MCP tools；Skill、Role 内容和 MCP 的上述热更新边界有
真实门；缺失任一 Role/Skill/MCP/tool 或发生 config warning/reload/status 失败时明确
unavailable，不产生或沿用 false readiness；Workspace 中没有 capability 文件。

### Slice 4：原生协同与 child elicitation

状态（2026-08-09）：4B.1 已完成 standard MCP typed form 的 child→Approval→原 child 恢复
闭环；4B.2-A 已删除 Work State/coordination 第二控制面、Supervisor snapshot/binding/
continuation、Event Projection 主动 continuation/Root Turn、provider speculative hashes 和
历史 SHA password compatibility；4B.2-B 已删除非 official capability manifest、
`profile_capabilities` 与 generated Web capability bundle 链。fresh PostgreSQL security gate、native
child projection 与 Workspace reuse gate 均通过。阶段一不再把 Browser legacy 清理、Platform
Provider 全量去重、首消息 compound selection 或完整 4B.3 Thread truth 当作仓网前置。它们保留在
后续 backlog，只有出现会直接使当前仓网链不可达、写错数据或泄露 Secret 的可复现失败时，才与
触发该失败的最小原子片一起修复。

当前唯一关键路径固定为：

1. **A — Codex Runtime 最小封口。** R2 真实 DeepSeek 门已在 Shared `de9cc23b4` 上通过：
   encrypted Secret restart、显式 model Turn、两次串行 MCP 调用与 final 均有真实证据；不借机
   实施 model refresh 或完整 R3。
2. **B — Server 实际适配。** 只完成通用 Copilot 运行对当前 Web Server 必需的 typed bridge 与
   owning-layer 适配，包括把官方 child Thread/Turn/Item 生命周期投影为可追踪的 Agent activity；
   不建立第二 Runtime、第二上下文、第二执行日志或固定业务流程。
3. **C — 通用可组合 Tools。** 统一 `supply_chain` provider 和单一 `ResourceRef`，完成 Data/Network
   可组合 active surface、route/cost pair-level 部分复用，以及 final map/report 的最小交付合同。
4. **D — 完整案例验收。** 把 Indonesia cold E2E 作为通用 Copilot 的一个完整组合示例，同时验证
   partial reuse 与跨 Workspace/未授权 ref denial；案例顺序不成为产品 workflow。
5. **E — 旧 caller 尾删。** D 通过后再原子删除 `CaseRepository`/`NetworkSnapshot` 等旧 caller，
   不保留兼容双路径。

R2 真实门退出：fresh local session 可添加并列出 DeepSeek Provider；配置只保存 env ref，Platform
Secret 重启后仍能注入同一密钥；显式 `deepseek-v4-flash` Turn 完成连续两次 MCP tool call 后给出
final。浏览器响应、日志、Workspace 与普通 Profile 文件均不得出现 Secret 明文。模型 refresh、
`forceRefresh`/`providerId` model-list 扩展、Turn Provider override、Provider DB/UI 全量重构和
Browser dead graph 均不属于该门。

后续 backlog 继续遵守已确认边界：official Thread/Turn/Item/history 是唯一会话事实；Browser legacy、
Terminal/Usage/prompts、Provider 重复 owner、Task creation selection、Run lease/history overlay 与
physical-cwd join 必须按 owner 原子收敛，但不得再次插到上述仓网关键路径之前。

通用 Copilot infrastructure 的阶段一内嵌实现不改变 A→E 顺序：当前只要求它集中、领域无关，
由 Data、Network 与 final Tool 共用。出现第二个真实 Copilot，或进入 phase-two public Copilot
SDK 之前，必须把 ResourceRef/schema/codec/bounds/error/provider primitives 迁到平台提供的通用
provider library，把 Workspace authority/writer/Artifact 物化留在 Platform/Runner；迁出时保持
现有 ResourceRef 与 Tool 合同不变，MCP provider 继续拥有 Resource 字节和生命周期。

Indonesia 完整验收可以组合为：Root 确定国家和目标；Network child 定义数据要求；Data child
检查与准备文件；Root 使用 native wait/mailbox 并保持可与用户互动；再由 Network child 完成
所需分析并由 Root 综合交付。这只是覆盖完整能力的一种验收组合，不是固定 Supervisor workflow；
实际 Agent、工具和先后顺序由用户目标、当前上下文与 Skill 动态决定。

当前用三个彼此独立的产品样例证明这一点：

1. **数据理解与准备。** 只使用授权 Workspace 中的用户数据，完成检查、映射、标准化与必要的
   地理信息准备；不启动 Network 分析。
2. **Indonesia 完整组合。** 在用户明确选择示例后完成 baseline，引入两个候选仓，计算分配、
   6/12/18 小时覆盖、成本，并生成地图和报告；验收业务结果与交付，不规定 Tool 顺序或固定数值。
3. **最短场景子链。** 复用同一份已准备数据，只关闭 `WH-CROSS_DOCKING-BEKASI`，比较 12 小时
   覆盖、成本与重分配；不重新运行选址、地图或报告。

样例 1 与样例 3 是轻量回归，样例 2 是 Stage D 收口门。任何实现都不得为了满足样例而把国家、
仓库 ID、调用顺序或场景步骤写进 Platform workflow、Tool 状态机或 Supervisor 固定流程。

1. child 发出的 requested schema 投影为统一业务卡片，答案直接回到原 child request。
2. 覆盖提交、拒绝、取消、超时、中断、校验错误和刷新恢复；UI 不暴露 child Thread ID、raw
   MCP request/response、app-server request ID 或未裁剪 schema。
3. Agent activity 以官方 child Thread/Turn/Item 为唯一事实源，展示 Agent 身份、任务、Item
   类型、MCP server/tool、bounded 参数与结果摘要、经安全裁剪的 command/action/output 摘要、
   started/completed/failed/waiting/approval 状态和时间顺序。不得继续把所有 commandExecution
   压成 `Using/Completed a workspace command`，也不得只保留“数据检查”“路线与成本计算”之类
   无法区分真实步骤的业务占位文案。Browser 可以按 Agent/Turn 展开官方 Item 明细，但不得显示
   server absolute path、Secret、未裁剪输出、raw JSON-RPC/request ID 或 MCP Resource 内容。
4. live WebSocket、刷新后的 durable projection 与 official paginated child history 必须收敛到同一
   Item 身份和终态；乱序 started/completed、child 先于 SubAgentActivity Started、审批等待、失败、
   取消、中断和 replay 均不得产生重复、丢失或伪造步骤。Platform 不复制 Thread history，只保存
   可重建的 bounded UI projection。
5. 已删除 Work State/coordination MCP 与 gates、`needs_input` 中继、event projection 的仓网
   continuation Prompt、主动发 Turn、Data gate global interrupt 和第二调度器路径。
6. 已删除 Supervisor policy snapshot/binding 与 continuation 表/调度；Runtime history 和 exact
   Item 仍是协作事实 owner。

退出：Root 在 child 运行时仍可响应用户；child success/error/cancel 均由原生终态和 mailbox
返回；Root final 后 mail 不自动启动新 Turn；Profile 重启时 pending form 明确 interrupted；
Web Agent activity 可实时展开并在刷新后恢复同一 child Item 序列，实际 MCP Tool、command/action、
progress、approval 与 terminal outcome 可区分，且没有第二 Thread/history/log owner。

### Slice 5：原生 Resource 与 Workspace 文件驱动的完整仓网能力

仓网规则全部进入 Skills 与确定性 Tools，不进入 Platform DTO、数据库或状态机。

#### Active surface 切换硬门

Data/Network 新 Resource 工具进入 Role allowlist、真实调用或 E2E 之前，必须删除 generic
`ResourceLink→Artifact` 注册、任意 renderer 注册、跨 child Resource 搜索和 Artifact 输入回流，
否则每个中间 profile、mapping、matrix、scenario Resource 都会被 Platform 二次物化为 Artifact，
形成第二数据面。对话内地图只保留一条窄展示合同：exact `map_utils/create_map_card` 返回 bounded
`map.v3` renderer，Assistant 原样输出其独立行 `::codex-inline-vis{...}` embed；Platform 只保存
renderer 与 producing Thread 的精确 Resource ref，并在浏览器授权读取时调用 official
`mcpServer/resource/read`，不保存 Resource 字节、不登记 durable Artifact。final Artifact 子片尚未
就绪时，导出/下载显式 unavailable；不得借地图卡片冒充文件交付。

#### 数据准备

1. Network Agent 定义需求列表与已有仓列表的必要字段。
2. Data Agent 读取用户指定的 Workspace Excel/CSV/JSON；多个候选文件时用业务语言询问，
   不按时间或文件名猜“最新”。
3. 推断字段并映射数据要求；缺字段或映射歧义时用通俗业务语言解释并询问。
4. 获取国家行政区 city/province ID/name 与经纬度，为需求城市、已有仓和候选仓补坐标。
5. 有候选仓文件则使用；没有则询问省级、市级或用户指定范围。
6. 按用户与 Skill 要求，把需用户看到或跨 package 交接的结果写成 Workspace 文件，
   把 provider-owned typed intermediate 保存为 MCP Resource；是否保存和如何使用由用户与 Skill 决定。

#### 距离、成本与分析

1. 构建已有仓/候选仓到需求城市的距离与时长；询问曲面距离×绕路系数或导航。缺绕路系数
   时询问；导航前展示路线数量、接口消耗和费用风险并取得许可。
2. 优先使用用户路线报价；缺失路线时询问计价逻辑，禁止以零成本填补。
3. 按成本优先或时效优先计算覆盖；计算一个或多个 SLA 目标的满足率。
4. 计算全网和分仓运输成本，完成增仓、减仓、搬迁三类模拟。
5. 执行 p-median：明确 `p`，已有仓默认固定；只有用户明确许可时才允许指定已有仓关闭。
6. 执行给定 SLA 下的成本最优规划，输出覆盖、时效、距离、成本和仓库变动。
7. 生成地图、明细表和最终报告。
8. 路线按起终点坐标/身份、计算方法、provider 参数和 Tool 版本的 exact pair fact 复用；
   成本按路线 fact、报价/规则、币种和 Tool 版本的 exact lane fact 复用，只补算缺失项。

#### 文件与复用规则

- Tool 从 native `sandboxCwd` 解析 Workspace，相对路径是唯一文件定位合同。
- Task B 可读取 Task A 已写入同一 Workspace 的普通文件，或使用同一授权 provider 中
  由官方 Item/用户显式选择的精确 Resource ref；没有 Task A→Task B context/result API，Artifact 不作输入。
- Web Resource selector 从已授权 producer event 的 exact Item provenance 返回 bounded exact
  ResourceRef。Browser 选择时提交 producer event ID、ordinal 和 exact ref；Server 再次校验
  user/organization/Profile/Workspace、event/ordinal 与 `{server, uri}` 全字段相等。普通 RunEvent
  WS/HTTP 不广播 raw URI，不建立 opaque handle、latest alias、dedupe registry 或新投影表。
- Tool 根据真实输入和用户要求最大化复用，可以复用部分 pair/行并只补算缺失部分；
  Platform 不以 SHA 或整个数据版本限制复用。
- 文件名和目录由用户与 Skill 决定；默认 create-new，同名时询问覆盖、改名或取消。
- Mock 只在用户明确选择“使用印尼仓网完整示例”时把 fixture 作为普通可见文件放入干净
  Workspace，绝不能在真实输入失败后静默回退。

Stage C 的新 active surface 不再依赖 `CaseRepository`、NetworkSnapshot、DatasetRelease/profile
SQLite 黑板、`source_ref` 及 wire aliases、模型可见 revision/operation/CAS、ArtifactRef、
taskEvidence hashes、双重 ref、隐藏 Task 数据目录和 Artifact 数据回流；保留并简化 ResourceStore
为 MCP provider 的最小内容 owner，重用纯解析、验证、标准化、算法和 renderer 代码。Stage D
完整验收通过后，Stage E 再原子尾删这些旧 caller 和实现。

退出：所有仓网 Tool 只使用 Workspace 相对路径、typed MCP Resource ref 或普通业务参数；
完整仓网清单和 pair-level 部分复用都有确定性测试；两个 Task 可显式复用文件或 Resource
但没有直连 context/result API；Platform 不出现仓网状态机。

### Slice 6：最小 Artifact 与安全投影

1. Artifact 仅承担用户明确要求导出、下载或保留的最终地图、报告和交付件；结构化中间数据由
   Workspace 普通文件或 MCP Resource 拥有。只要求“展示/看看/可视化”时使用对话内地图卡片，
   不生成网页、PNG、Workspace JSON 或 durable Artifact。
2. 最终交付至少包含覆盖关系明细、城市对应仓/距离/时长/成本、SLA 覆盖率、总成本与分仓
   成本、模拟差异、p-median 结果与仓变动、地图和可下载报告。
3. Artifact 状态只投影真实的生成中、已完成、部分完成或失败；模型文本不能冒充完成。
4. built-in final map/report Tool 使用 exact `(server, tool)` allowlist 和 typed output schema，原子
   create-new 写一个自包含 Workspace-relative JSON bundle。Platform 从 producing Run 解析授权
   Workspace，复用 `GitRuntime::download_file` 的 relative-path、regular-file、no-follow、physical
   containment 与 100 MiB 边界，即时复制到 Artifact；中间 MCP Resource 永不注册 Artifact。
5. Artifact delivery 直接附着在 producing official completed Tool Item，并以 Item provenance 做
   replay/idempotency；Browser 可在该 Tool 后或交付面板展示，不等待 Assistant 复述文本。
6. Runtime tool/child 事件转为安全业务 DTO；按 thread identity 幂等、容忍乱序、可重放，
   不暴露本地绝对路径、raw MCP payload 或 app-server request ID。

当前旧 Artifact rows 都由已否决的 generic 注册产生，不做兼容迁移。新增 current-schema migration
按外键顺序删除 `inline_visualization_artifacts` → `artifact_task_grants` →
`artifact_provenance` → `artifacts`，再只重建后三张表：Artifact source 改为产生时解析出的
Workspace ID 与相对路径，ready 后的内容不依赖 Run/Workspace 存活；provenance 对 exact
producer Item 唯一，Task grant 仍只负责授权。删除 `retention_state`、`source_server/source_uri`
及其 URI dedupe；没有用户 delete/expiry owner 前 Artifact 默认持久。物化 worker 接收现有
`GitRuntime`，不再通过 Codex `read_mcp_resource` 或 producer Run→Thread 恢复内容。

退出：刷新后对话地图卡片可由 Runtime history、bounded renderer/ref 投影和 provider Resource
恢复；明确导出的地图和报告仍可查看下载；中间矩阵不进入 Artifact 交换链；durable 交付可从
Runtime history 和 Artifact 权威记录重建浏览器视图。

### Slice 7：双 E2E、恢复和全量清理

#### 硬门 A：三个可组合产品样例

- 样例 1 只完成 Workspace 数据理解与准备，明确断言没有调用 Network Tool。
- 样例 2 由用户明确选择 Indonesia fixture，完成 baseline、两个候选仓、分配、6/12/18 小时
  覆盖、成本、地图和报告；fixture 作为普通 Workspace 文件使用，Tool 顺序和业务数值不被
  写成产品合同。
- 样例 3 复用样例 2 的已准备数据，只关闭 `WH-CROSS_DOCKING-BEKASI` 并比较 12 小时覆盖、
  成本与重分配；明确断言没有重新运行选址、地图或报告。
- 三个样例都不使用隐藏对象或特殊数据 API，不注入 Task/Run/WorkState/ref/wire ID；各自实际
  经过的行数、路线数、业务结果、结果文件和交付件有确定性断言。

#### 硬门 B：真实上传交互完整链

- 用户通过通用 Workspace 文件面板上传 Excel/CSV/JSON，并在 Task 中要求 Copilot 使用。
- 覆盖字段映射、缺失数据、候选仓、距离方式、绕路系数、导航费用许可、成本规则、覆盖目标、
  SLA、模拟、p-median 和允许关闭已有仓等原生交互。
- 导航合同使用确定性测试 provider；另设配置真实导航凭据时的 smoke gate，未配置不能冒充成功。

#### 多 Task 与多 Workspace

- 同一 Workspace 的 10 仓和 5 仓 Task 可读取相同普通文件并按 exact Resource ref 复用中间数据，
  参数、新结果和 Artifact 互不覆盖；
  一个失败、取消或中断不影响另一个。
- Task A 按用户要求写入 Workspace 的结果文件可被后续 Task 读取；未写入的 context/结果不可得；
  Artifact 不能作为输入；同名保存触发显式冲突处理。
- Indonesia 与 Thailand Workspace 完全隔离；Platform 不理解国家语义。

#### 恢复与清洁门

- Root wait 期间 user steer、child error/cancel/terminal mailbox、pending form 刷新、Profile
  restart interrupted、事件 replay、文件删除/替换、单文件原子写失败、hot reload 成功/失败。
- 三个 Web 自然语言样例都必须能从 Agent activity 观察实际 child Item 序列：Data/Network Agent
  的 MCP Tool、bounded command/action、进度、等待/审批和终态可区分；刷新后顺序与终态一致，
  不出现 `Using/Completed a workspace command` 这类替代全部执行内容的占位投影。
- clean DB + clean Profile + clean Workspace 从当前 schema/config 完整初始化。
- 旧 Intake、Dataset、SourceAsset、DomainResource/Broker、Case、Work State、continuation、scan 和长 Prompt
  E2E 的代码、migration、fixture、测试与当前文档全部删除。

## 5. 删除与切换纪律

每个切片必须在同一 change 更新全部 caller、schema、fixture、测试、UI 和当前文档，然后删除
被替代路径；禁止双读、双写、字段猜测、deprecated DTO、alias、隐藏 fallback 或仅供旧 E2E
的成功分支。

阶段一最终不得保留；其中 4B.2-A 已完成的项标为“已删除”：

- 已删除：Work State service/routes/MCP/schema/migrations 与 Platform coordination MCP；
- Data Intake draft/session/mapping/gate/hold、SourceAsset 专用存储/API、DatasetRelease、
  DomainResource、Resource Broker 和 Task dataset/attachment/snapshot/projection；
- WorkspaceDataObject/revision/head/read binding、TaskSourceAsset、Task file store、semantic
  fingerprint/cache、typed publish/adopt；
- CaseRepository、NetworkSnapshot、DatasetRelease/profile SQLite 黑板、ArtifactRef、taskEvidence
  hashes、`source_ref` 与兼容 aliases；ResourceStore 仅保留 provider 内容存储必需部分；
- 已删除：event projection continuation、主动发 Turn、global interrupt；Slice 5 active surface
  切换前删除 generic ResourceLink→Artifact、inline/model-text 展示链、跨 child Resource lookup
  和 Artifact 输入回流；
- 已删除：Supervisor policy snapshot/binding/continuation、provider speculative hashes 和历史 SHA password 兼容；
- capability path scanning、假 installation/DB-only readiness，以及长 Prompt/手建 Work State E2E。

历史开发数据只通过 ADR-012 的显式重建流程清理，不能在 startup 猜测、迁移或兼容。

## 6. 验证与完成定义

验证必须覆盖正常、拒绝、取消、失败、中断、刷新、重启、乱序、并发和跨 Workspace 拒绝。
按受影响 owner 使用：

- `scripts/codex-upstream-status.sh`、`scripts/codex-customization-status.sh` 和
  `scripts/test-codex.sh`；
- `scripts/test-web-rust.sh`、Web lint/typecheck/Vitest/build；
- official Codex `schema_fixtures` drift gate 与真实
  `npm run smoke:codex-app-server` 的 `initialize`/`thread/list` 验证（协议变化时）；
- 仓网 Python pytest/Ruff、真实 stdio inventory/tool call、Mock E2E、真实上传 E2E 和配置真实
  导航时的 smoke gate。

阶段一只有在以下全部成立时才完成：

1. Web 的印尼 Mock 零询问完整链通过。
2. Web 的真实上传交互完整链通过。
3. 同 Workspace 多 Task 文件与授权 Resource 复用、不同 Workspace 隔离通过。
4. Task 之间不存在消息、context、result、data 或 adopt API。
5. Runtime 原生 discovery/reload、多 Agent、elicitation、history 与 Workspace metadata 被真实使用。
6. 完整仓网业务清单和最终交付件全部通过确定性断言。
7. 旧重型路径已从代码、数据库、测试和当前文档彻底删除。
8. clean DB/Profile/Workspace 全量重建成功。

没有成功 evidence 时，只更新能力基线中的缺口；不得用模型最终文本、数据库 success 字段、
Mock fallback 或旧 7/7 脚本声称完成。

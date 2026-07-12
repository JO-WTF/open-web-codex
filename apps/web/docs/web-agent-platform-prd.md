# CodexMonitor Codex Web Harness V1 产品需求文档

## 文档信息

| 字段 | 内容 |
| --- | --- |
| 文档状态 | Draft，作为产品与研发评审基线 |
| 产品版本 | V1.0 |
| 更新时间 | 2026-07-11 |
| 目标形态 | 单组织、多用户、自托管 Codex Web Harness |
| 客户端 | 标准浏览器，不要求桌面客户端或本地 Agent |
| Agent Runtime | 服务器 Codex Host 中安装的 Codex CLI `app-server` |
| 上游依赖 | 并行 Codex Rust 改造项目提供缺失的 app-server 桥接 |
| 配套计划 | `docs/web-agent-platform-development-plan.md` |

## 1. 文档目的

本文档定义 CodexMonitor 从 Tauri 桌面应用迁移为多用户 Codex Web Harness 后的产品范围、用户模型、功能需求、权限、安全边界、上游依赖、服务指标和验收标准。本文档是 V1 产品设计、Codex Rust 桥接、Web 研发、测试验收和发布决策的统一依据。

## 2. 产品摘要

CodexMonitor Web 是面向研发团队的 Codex Web Harness。用户通过浏览器使用 Codex 原生的 Thread、Turn、多 Agent 协作、记忆、Agents、Skills、Plugins、MCP、审批和代码执行能力，并完成代码审查、Commit 或 Push。

产品不重新实现 Agent 规划、子 Agent 调度、上下文压缩、记忆整合、Skill Runtime、Plugin Runtime 或 MCP Tool Runtime。浏览器不直接调用 Codex CLI；Web Server 负责用户、项目、权限、Profile 映射与审计，Codex Host 负责持久 Codex Profile、app-server 生命周期、Workspace 注册和协议转发。

V1 面向可信团队的单组织部署。数据库和权限模型保留 `organization_id`，但不承诺公共多租户 SaaS 所需的计费、跨组织资源调度和强租户合规能力。

### 2.1 V1 核心架构

```text
Browser Web Client
  -> Web Server / Auth / Project ACL
  -> Codex Host Gateway
  -> Persistent Codex Profile
  -> codex app-server
  -> Threads / Multi-Agent / Memory / Skills / Plugins / MCP
  -> Per-Task Git Worktrees
```

Codex Profile 是运行时隔离和持久化单位。一个用户默认绑定一个个人 Profile；Profile 拥有独立的 Codex 身份、`CODEX_HOME`、配置、Agent 定义、全局 Skills、Plugins、MCP 配置、Prompts、Thread 历史和记忆数据。一个 Profile 在同一时刻最多运行一个主 app-server 实例，该实例可注册多个授权 Workspace 并承载多个 Thread。

Task/Run Workspace 是代码隔离单位。每个 Run 在所属 Task 下使用独立 Git Worktree；同一 Task 的恢复 Run 可以创建后继 Worktree，但任何可写目录都不与其他 Task 共享。创建 Workspace 不创建新的 Codex Home，app-server 重启后必须使用同一 Profile 恢复 Thread、配置和记忆。

### 2.2 Agent 事实来源

| 数据 | 事实来源 | 平台职责 |
| --- | --- | --- |
| Thread、Turn、Agent Item、上下文压缩 | Codex app-server/Profile | 映射、展示、权限过滤和必要缓存 |
| 多 Agent 分工、深度、并发与协作消息 | Codex Agent Runtime | 配置入口和状态可视化 |
| 个人记忆与 Memory Consolidation | Codex Profile | 持久卷、生命周期和重置控制 |
| 项目指令 | 仓库 `AGENTS.md` | 只读展示、受控编辑和版本审计 |
| 个人 Skills/Plugins/MCP | Codex Profile | Web Studio、校验、安装和权限控制 |
| 项目 Skills | 仓库 `.agents/skills` | 创建、评审、Git 版本化和发布 |
| 用户、组织、项目权限、Task 映射 | 平台 PostgreSQL | 唯一事实来源 |
| Git Workspace、Commit、Push | Git/Worktree | 生命周期、权限和审计 |

平台不得创建第二套 Agent Thread、Memory Summary 或多 Agent 调度状态机。数据库可以保存索引、Task 到 Thread 的映射和审计快照，但恢复 Agent 上下文必须优先调用 Codex。

### 2.3 目标满足结论与成立前提

本方案可以满足“只建设 Web 用户客户端，同时复用 Codex 多 Agent、记忆、Skills、Plugins 和工具体系”的 V1 目标，但成立条件不是“浏览器直接复用 CLI”，而是以下四项同时成立：

1. 服务端部署 Codex Host 和受控的 Codex Rust/app-server 构建，浏览器只依赖平台 API 与 WebSocket。
2. Codex Rust 改造项目补齐并稳定暴露本 PRD 所需的 Profile、Agents、多 Agent 事件、Memory、Skills、Plugins、MCP/OAuth/elicitation bridge。
3. Web 项目坚持 Harness 边界，只建设多用户、权限、Profile 生命周期、Workspace/Git、审批、UI 和审计，不重写 Codex Agent 逻辑。
4. Capability Manifest、Schema、Fixtures、真实 Smoke Test 和版本兼容门禁成为两个项目的强制发布合同。

满足这些前提后，Tauri 和桌面客户端可以完全删除，最终用户只需浏览器。若某项 Rust bridge 未完成，相应 Web 模块必须保持不可用而不是使用功能不等价的替代实现；因此 V1 范围和 GA 日期受并行 Rust 里程碑约束。

## 3. 背景与现状

现有产品是 React + Tauri 桌面应用，具备 Workspace、Thread、消息、审批、Diff、Git、Prompt 和 Codex app-server 协议处理能力。当前 Web 分支仅提供基于共享 Token、HTTP RPC 和 SSE 的实验入口，不具备正式用户系统、权限模型、持久化事件、执行隔离或故障恢复能力。

V1 迁移必须保留有价值的 React 展示组件和 Rust 领域逻辑，同时移除以下桌面假设：

- 一个使用者拥有整台运行机器。
- Workspace 可以由用户输入任意本地路径。
- 所有用户的 Workspace 可以在无 Profile 授权边界下共享同一 Codex 进程和凭据。
- 浏览器状态和 `localStorage` 可以作为业务事实来源。
- 原生窗口、托盘、更新器、Finder/Explorer 和桌面通知始终可用。

## 4. 产品愿景与原则

### 4.1 产品愿景

让团队能够在一个可审计、可协作、可恢复的 Web 工作台中安全地使用 Codex 完成真实代码任务。

### 4.2 产品原则

1. Web 优先：所有核心流程仅使用浏览器即可完成。
2. 事实来源清晰：平台服务端管理用户、Task 映射、审批和代码治理；Codex 管理 Agent Thread、Turn、事件语义和记忆。
3. Profile 隔离：不同用户 Profile 不共享 Codex Home、凭据或 app-server；同一 Profile 内跨 Task 复用 Codex 记忆和配置。
4. 人在回路：高风险命令、文件变更和权限请求必须进入可追踪审批。
5. 可恢复：页面刷新、网络重连和 Server 重启不能造成任务状态永久不一致。
6. 渐进扩展：V1 使用模块化单体与 PostgreSQL，达到规模阈值后再拆分队列和 Worker 集群。
7. 协议封装：Codex app-server 协议只存在于 Codex Host 适配层，不成为公开 Web API。
8. Codex 原生优先：能由 Codex Runtime 提供的 Agent、Memory、Skill、Plugin 和 Tool 能力不在平台重新实现。
9. 能力协商：Web 功能由上游 Capability Manifest 和 Schema 驱动，不假定所有 CLI 版本能力一致。

## 5. 产品目标与非目标

### 5.1 V1 目标

- 支持邀请制多用户登录和组织成员管理。
- 支持基于 Git URL 导入仓库并创建项目。
- 支持创建、排队、运行、继续、取消和归档 Agent 任务。
- 支持 Codex 消息、计划、工具调用、命令输出、Diff 和状态实时展示。
- 支持命令、文件变更、权限提升和用户输入请求的 Web 审批。
- 支持每个用户独立且持久的 Codex Profile、Codex Home 和 app-server 生命周期。
- 支持每个 Run 在 Task 边界内使用独立 Worktree，并在同一 Profile 中复用 Thread、记忆和配置。
- 支持 Codex 原生多 Agent 的创建、配置、运行状态和协作过程展示。
- 支持在 Web 中创建、验证、测试、发布和回滚个人/项目 Skills。
- 支持 Plugin 安装、读取、更新、禁用和卸载，以及权限影响预览。
- 支持 MCP Server 配置、Reload、OAuth、Elicitation、状态和测试调用。
- 支持 Profile 记忆连续性验证、重启恢复、导出和受控重置。
- 支持任务级成员访问、控制者租约和审计日志。
- 支持 Diff 审查、Commit 和 Push 的完整闭环。
- 删除 Tauri、桌面安装包和桌面客户端运行依赖。

### 5.2 V1 非目标

- 公共注册、公开市场或面向匿名用户的 SaaS。
- 计费、订阅、发票和按组织收费。
- 跨地域多活和零停机容灾。
- 让浏览器操作用户个人电脑上的本地仓库。
- 完整在线 IDE、任意文件编辑器或浏览器内调试器。
- Kubernetes 作为首版强制依赖。
- 承诺任意 Codex CLI 版本均兼容。
- 自建多 Agent Planner、子 Agent Scheduler、Memory Engine 或 Skill Runtime。
- 在平台数据库中复制并替代 Codex Thread/Memory 作为 Agent 事实来源。
- 在 Web 项目中自行实现 Codex Rust 项目已经提供的 Plugin、Skill、MCP 或 Memory 协议逻辑。
- 保留 Tauri 桌面版、iOS 客户端或桌面自动更新。

## 6. 用户与角色

| 角色 | 核心诉求 | 默认能力 |
| --- | --- | --- |
| 组织所有者 | 管理团队、安全和平台配置 | 全部组织权限 |
| 项目管理员 | 管理仓库、成员和执行策略 | 项目配置、成员和任务管理 |
| 开发者 | 使用 Agent 完成代码任务 | 创建任务、发送指令、审查与提交 |
| 审核者 | 审查执行过程和代码变更 | 查看、评论、处理授权范围内审批 |
| 只读成员 | 了解任务进展和结果 | 只读访问项目、任务、Diff 和审计摘要 |
| 平台管理员 | 维护部署、Runner 和系统安全 | 系统状态、Runner、配额和全局审计 |

## 7. 核心术语

| 术语 | 定义 |
| --- | --- |
| Organization | 用户与项目的权限边界；V1 每个部署启用一个组织 |
| Project | 一个代码仓库及其成员、策略和执行配置 |
| Task | 用户可见的长期工作单元，可包含多个 Run |
| Thread | Codex 对话上下文，与 Task 建立稳定映射 |
| Run | Task 中一次可执行、可终止、可审计的 Agent 运行 |
| Codex Profile | 用户级持久 Agent Runtime，包含身份、`CODEX_HOME`、配置、Agents、Skills、Plugins、MCP、Threads 和 Memory |
| Execution Policy | 模型、推理、Sandbox、审批、网络和环境等运行默认值；属于 Profile 或项目配置 |
| Workspace | 某个 Run 在所属 Task 边界内的隔离文件系统与 Git Worktree |
| Codex Host | 管理 Codex Profile、app-server 进程、Workspace 注册、能力协商和协议转发的服务端组件 |
| Runner | 管理 Git Workspace、容器、资源、命令和 Artifact 的服务端执行组件，可与 Codex Host 同进程部署 |
| Capability Manifest | Codex Rust app-server 暴露的版本化能力清单，决定 Web 可以启用的模块和操作 |
| Control Lease | 同一 Task 在某一时刻允许一个用户发送控制指令的短期租约 |
| Approval | Codex 发出的命令、变更、权限或人工输入请求的持久化记录 |
| Artifact | 日志、测试报告、补丁、附件或其他运行产物 |

## 8. 信息架构

主导航按实际工作流组织：

1. 工作台：我的任务、运行中、待审批、最近项目和异常任务。
2. 项目：仓库、任务、成员、执行配置和项目设置。
3. 任务：对话、运行状态、计划、活动、审批、Diff、文件和终端输出。
4. 审批中心：当前用户有权处理的待审批请求。
5. Codex Studio：Profile、原生 Agents、Skills、Plugins、MCP/Tools 和能力状态。
6. 团队设置：成员、角色、邀请和安全设置。
7. 平台管理：Runner、队列、容量、审计和系统健康。

任务详情使用三块工作区：左侧任务导航，中间对话与活动流，右侧 Diff、文件、日志和运行详情。窄屏使用 Tabs 切换；移动端优先支持观察、回复与审批，不要求完成复杂 Diff 编辑。

### 8.1 路由与导航结构

| 路由 | 页面 | 访问条件 | 主要任务 |
| --- | --- | --- | --- |
| `/login` | 登录 | 未登录 | 登录、处理邀请、恢复会话 |
| `/onboarding` | 初始化向导 | Owner 且组织未初始化 | 配置组织、Codex Profile、Git 和首个项目 |
| `/dashboard` | 工作台 | 已登录 | 发现待处理事项、恢复最近任务、创建任务 |
| `/projects` | 项目列表 | 已登录 | 搜索、筛选、创建和进入项目 |
| `/projects/:projectId` | 项目概览 | 项目成员 | 查看活跃任务、仓库状态、成员和容量 |
| `/projects/:projectId/tasks` | 项目任务 | 项目成员 | 按状态、成员、分支和时间筛选任务 |
| `/projects/:projectId/settings` | 项目设置 | Project Admin | 仓库、成员、执行、审批和保留策略 |
| `/tasks/:taskId` | 任务工作区 | Task 可见 | 对话、控制、审批、Diff、日志和交付 |
| `/approvals` | 审批中心 | 有审批权限 | 集中处理待审批、已决和过期请求 |
| `/codex/profiles` | Codex Profiles | 已登录 | 连接身份、查看健康与能力、重启和恢复个人 Profile |
| `/codex/agents` | Native Agents | Developer 以上 | 管理 Codex 原生 Agent 与多 Agent 参数 |
| `/codex/skills` | Skills Studio | Developer 以上 | 创建、验证、测试、发布和回滚 Skills |
| `/codex/plugins` | Plugin Manager | Developer 以上 | 查看、安装、升级、停用和卸载 Plugins |
| `/codex/mcp` | MCP 与 Tools | Developer 以上 | 配置连接、OAuth、授权、测试和健康状态 |
| `/settings/profile` | 个人设置 | 已登录 | 个人资料、通知、界面和会话 |
| `/settings/team` | 团队设置 | Owner | 成员、邀请、角色和组织安全 |
| `/admin/runners` | Runner 管理 | Platform Admin | Runner、队列、容量、故障与清理 |
| `/admin/audit` | 审计 | Owner 或受权管理员 | 查询关键操作和安全事件 |

### 8.2 全局 App Shell

全局 Shell 由主导航、页面标题区、内容区、全局通知和用户菜单组成。它是业务工具界面，不使用营销 Hero、装饰性大卡片或重复的模块说明。

- 主导航固定提供工作台、项目、审批和 Codex Studio；团队设置和平台管理根据权限显示。
- 导航项同时使用图标和文本；收起后仅显示图标并提供 Tooltip。
- 页面标题区只承载当前资源名称、面包屑、关键状态和页面级主操作。
- 搜索入口使用全局命令面板，支持按项目、任务 ID、标题和成员搜索。
- 全局通知抽屉展示待审批、运行完成、失败、租约请求和系统告警。
- 用户菜单提供个人设置、当前组织、会话信息和退出，不放置项目级配置。
- 浏览器离线、WebSocket 重连、权限变更和系统维护使用全局状态条，不以临时 Toast 代替持续状态。

### 8.3 页面层级与返回规则

- 从工作台进入 Task 后，返回操作回到进入前的任务列表及原筛选条件。
- 从项目页创建 Task 成功后直接进入 Task 工作区，不停留在成功弹窗。
- 从全局审批中心处理审批后保留列表位置，并提供“查看任务”链接。
- 深链接访问无权限资源时显示无权限页；不存在与无权限对普通成员使用相同外部表现，避免资源枚举。
- 浏览器前进和后退必须恢复 Tab、筛选、选中文件和滚动位置，但不得恢复已失效的写入表单。
- 所有可分享 URL 只包含稳定资源 ID 和非敏感视图参数，不包含 Token、凭据、Prompt 或文件内容。

## 9. 核心用户流程

本节工作流为产品、交互、接口和 E2E 测试的共同依据。每条工作流必须实现正常路径、列出的异常路径和明确终态。

### 9.1 WF-01 受邀用户首次进入

**前置条件：** 组织已初始化，邀请未过期且未被使用。

**正常路径：** 用户打开邀请链接，确认组织名称和受邀角色，设置账户凭据并登录。平台创建 Membership 和 Session，跳转工作台。若用户可访问项目，工作台显示最近项目；否则显示等待管理员分配项目的空状态。

**异常与边界：** 邀请过期时只允许请求管理员重新发送；邀请邮箱与已有账户不一致时必须重新认证；被禁用用户不能通过旧邀请恢复；邀请链接不得自动登录。

**终态：** 用户拥有唯一账户、有效 Membership、可吊销 Session 和一次 `member.joined` 审计记录。

### 9.2 WF-02 组织初始化

**前置条件：** 全新部署且不存在已完成初始化的组织。

**正常路径：** 首个管理员设置组织名称和时区，选择 Codex 身份模式，创建并验证 Codex Profile，配置 Git 凭据并创建首个项目。每一步保存服务端草稿，完成最终检查后激活组织。

**异常与边界：** Codex 验证失败时允许返回修改，不允许跳过后创建可运行项目；Git 凭据失败时可暂存组织但项目保持不可运行；初始化完成后入口永久关闭，后续修改进入设置页。

**终态：** 组织状态为 active，至少存在一个 Owner、一个可用 Codex Profile 和一个可选的可用项目。

### 9.3 WF-03 创建项目与连接仓库

**前置条件：** 操作者拥有项目创建权限，至少存在一个可用 Git Credential。

**正常路径：** 用户输入 Git URL 或从 Provider 选择仓库，选择凭据，执行连接测试，确认默认分支和项目名称，再配置成员、默认 Agent、并发与审批策略。平台异步建立只读镜像并显示进度。

**异常与边界：** URL 格式错误在客户端即时提示；DNS、认证、仓库不存在和分支不存在使用不同错误；镜像创建失败可重试且不重复创建项目；不允许普通用户输入服务器本地路径；重复仓库可以创建不同项目，但必须明确提示。

**终态：** 项目为 ready 或 setup_failed；只有 ready 项目可以创建 Run。

### 9.4 WF-04 创建 Task

**前置条件：** 项目 ready，用户具有 `task.create`，项目未达到禁止创建的配额状态。

**正常路径：** 用户填写任务目标，选择基线分支、可用的 Codex 原生 Agent、模型、推理等级和附件。高级配置默认折叠。提交前显示当前 Profile、执行策略、审批级别与能力兼容性。提交后立即创建 Task 与 queued Run，并进入任务工作区。

**异常与边界：** 任务目标为空或仅空白不可提交；附件必须先完成上传和病毒/类型检查；分支在提交期间消失时保留草稿并要求重选；重复点击通过幂等键只创建一个 Task。

**终态：** Task 可访问，Run 为 queued；用户成为 Task Owner 和默认 Control Lease 持有者。

### 9.5 WF-05 排队与 Workspace 准备

**前置条件：** Run 为 queued，关联项目、凭据和 Codex Profile 均可用。

**正常路径：** 调度器根据组织、项目和 Runner 容量领取 Run。任务工作区展示排队原因和顺序的近似信息。Runner 为 Task 创建独立 Worktree 和受控执行环境；Codex Host 确保关联 Profile 的 app-server 已启动并完成能力协商，再向该 Profile 注册 Workspace、创建或恢复 Codex Thread，最后把 Run 切换为 running。该流程复用 Profile 的持久 Codex Home，不为 Run 新建身份目录。

**异常与边界：** 容量不足保持 queued；凭据失效进入 failed 并提示管理员处理；Profile 不健康或能力不兼容时进入 blocked 并给出修复入口；用户在 provisioning 期间取消时必须终止后续步骤；创建了一半的 Workspace 必须进入清理流程，但不得删除持久 Profile 或其他 Task 的 Thread。

**终态：** Run 为 running、cancelled 或 failed，不能长期停在 provisioning；超时由后台巡检纠正。

### 9.6 WF-06 运行中交互

**前置条件：** Run 为 running，用户可查看 Task。

**正常路径：** 活动流按序展示 Agent 消息、计划、工具调用和代码变更。Control Lease 持有者可以发送 follow-up、steer 或停止请求；其他成员只能评论。新事件到达时，若用户位于底部则自动跟随，否则显示“有新活动”按钮并保持阅读位置。

**异常与边界：** WebSocket 断开后进入重连状态并使用游标补发；重复事件幂等合并；输出过大时折叠并转 Artifact；权限在运行中被撤销时立即禁用输入并关闭受限订阅。

**终态：** Run 继续 running、进入等待态或进入终态；界面不根据本地推断伪造完成状态。

### 9.7 WF-07 审批与人工输入

**前置条件：** app-server 发出可识别的审批或用户输入请求。

**正常路径：** 平台持久化请求并让 Run 进入 waiting_approval 或 waiting_input。任务内显示上下文卡片，审批中心显示同一记录。授权用户查看命令、路径、风险、影响范围和历史规则后同意或拒绝；人工输入支持结构化选项和自由文本。

**异常与边界：** 多人并发决策仅接受第一个合法结果；请求过期后按钮失效；Runner 失联后冻结请求；高风险审批不能由自动规则处理；拒绝理由按策略可设为必填。

**终态：** Approval 进入唯一终态，决策写入 app-server、Run 事件和审计；无法投递时 Run 进入 interrupted 而不是假装已响应。

### 9.8 WF-08 Control Lease 转移

**前置条件：** Task 至少有两名具备控制资格的成员。

**正常路径：** 非持有者点击“请求控制”，当前持有者收到站内提示并可移交。移交后旧输入框立即变为只读，新持有者获得发送能力。持有者离线超过租约宽限期后，系统允许符合权限的成员接管。

**异常与边界：** 当前存在未发送草稿时只保存在旧持有者本地；管理员强制回收必须填写理由；审批权限与 Control Lease 分离；多个接管请求按时间显示但不自动竞价。

**终态：** 同一 Task 始终最多一个有效控制租约，每次变化有事件和审计记录。

### 9.9 WF-09 故障、恢复与继续

**前置条件：** Run 出现 app-server 异常、Runner 失联、网络中断或平台重启。

**正常路径：** 平台先根据租约和心跳确认故障范围。浏览器网络故障只影响实时连接，不改变 Run；Profile app-server 退出时 Codex Host 使用同一 Codex Home 重启并尝试恢复 Thread；当前 Turn 无法确认时进入 interrupted。用户查看诊断后可恢复原 Thread 或基于原 Task 创建新 Run。

**异常与边界：** 不允许把未知状态自动标记 completed；恢复操作不复用旧请求 ID；新 Run 使用新 Workspace；旧 Workspace 在保留期内只读；恢复 Profile 不得把其他用户的 Thread、凭据或记忆加载进来。

**终态：** 每个 Run 有明确终态、故障来源和恢复入口，不存在无限 running。

### 9.10 WF-10 代码审查与交付

**前置条件：** Run 已产生 Git 变更，用户具有查看权限。

**正常路径：** 用户从任务右侧进入 Changes，按状态和路径筛选文件，逐文件审查 Diff 和测试摘要。用户可以继续要求 Agent 修改。完成后有权限用户打开 Commit Drawer，确认文件范围、分支和 Message，再执行 Commit；Push 需要独立确认。

**异常与边界：** 大文件和二进制只显示元数据；远端领先时禁止静默覆盖；Push 认证失败保留本地 Commit；保护分支禁止直接 Push；任务完成不自动 Commit 或 Push。

**终态：** 变更保持未提交、已提交或已推送之一，状态与 Git 实际结果一致并写入审计。

### 9.11 WF-11 任务归档与 Workspace 清理

**前置条件：** Task 没有 running 或 waiting 状态的 Run。

**正常路径：** 用户归档 Task 后，它从默认列表移除但保留历史。Workspace 到达保留期后进入清理，Artifact 按保留策略独立处理。用户在清理前可导出 Patch 或查看诊断。

**异常与边界：** running Task 不允许归档；存在未 Push Commit 时必须明确提示；清理失败进入管理员队列；恢复归档 Task 不恢复已删除 Workspace。

**终态：** Task 可恢复查看，Workspace 和凭据按策略安全释放。

### 9.12 WF-12 Runner 运维

**前置条件：** 用户为 Platform Admin。

**正常路径：** 管理员查看 Runner 心跳、CLI 版本、容量和运行列表，可暂停接单、排空节点或终止异常 Run。升级时先暂停接单，等待运行完成，再替换镜像并通过 Smoke Test 恢复。

**异常与边界：** 强制终止必须二次确认；删除 Runner 不删除历史 Run；版本不兼容节点自动标记 unavailable；批量操作需要显示影响数量和失败明细。

**终态：** Runner 状态、调度资格和受影响 Run 可被审计和追踪。

### 9.13 WF-13 Codex Profile 创建、重启与恢复

**前置条件：** 用户已登录且具有 Profile 管理权限，平台存在兼容的 Codex Host。

**正常路径：** 用户在 Codex Studio 创建个人 Profile，选择受支持的身份方式并完成服务端认证。Codex Host 为其分配持久且隔离的 Codex Home，启动唯一主 app-server，读取 Capability Manifest，并展示版本、身份、原生配置、记忆状态和已注册 Workspace。用户可在无活动 Turn 时执行健康检查或安全重启；重启必须复用同一 Codex Home，并通过原 Codex Thread 验证连续性。

**异常与边界：** 同一 Profile 的并发启动请求必须收敛为一个主进程；认证信息不得返回浏览器；版本不兼容时 Profile 标记 incompatible；有活动 Turn 时普通重启需要确认并将受影响 Run 标记 interrupted；重置记忆、退出身份和删除 Profile 分别处理，不能合并为一个模糊的“清空”操作。

**终态：** Profile 为 ready、degraded、incompatible、auth_required 或 stopped 之一；生命周期、操作者、版本和恢复结果有审计记录。

### 9.14 WF-14 原生 Agent 与多 Agent 协同

**前置条件：** Profile ready，Capability Manifest 声明原生 Agent 配置和 multi-agent 可用。

**正常路径：** 用户在 Native Agents 中创建或编辑 Codex 原生 Agent，配置说明、模型和可用工具，并设置 `multi_agent_enabled`、`max_threads` 与 `max_depth`。创建 Task 时选择该 Agent；运行中由 Codex 自己规划和调度子 Agent。活动流按父子 Thread 展示派生、委派、等待、返回和汇总事件，用户可以查看每个子 Agent 的状态与产出，但平台不参与调度决策。

**异常与边界：** 超出 Codex 原生深度或线程限制时展示 app-server 返回的真实错误；子 Agent 失败不自动推断主 Run 失败；关闭 multi-agent 只影响后续 Turn；平台不得以 Web Job 或自定义队列模拟缺失的子 Agent 能力。

**终态：** Agent 配置由 Codex Profile 持久化，Task 记录所用配置快照和 Codex Thread 映射，多 Agent 轨迹可恢复、可审计且不被平台改写。

### 9.15 WF-15 Skill 创建、测试、发布与使用

**前置条件：** 用户具有目标作用域写权限，Capability Manifest 声明所需的 Skill 读写、校验和刷新能力。

**正常路径：** 用户选择个人或项目作用域，从模板创建或要求 Codex 生成 Skill 骨架，在编辑器中维护 `SKILL.md` 及允许的 `scripts`、`references`、`assets`。平台调用 Codex 原生校验，随后在隔离测试 Task 中运行样例并展示实际工具调用与结果。个人 Skill 发布到 Profile；团队 Skill 通过受控变更写入项目 `.agents/skills` 并进入 Git 审查。发布后刷新能力并在新 Turn 中验证可发现性。

**异常与边界：** 不允许绕过路径、大小、文件类型和权限校验；测试失败不允许标记已验证；项目 Skill 冲突必须走 Git 冲突流程；已被运行引用的版本保留快照；平台只编排编辑和发布，不实现 Skill 解释器。

**终态：** Skill 具有明确作用域、来源、版本、校验结果、测试记录和发布状态，可停用或回滚到可追踪版本。

### 9.16 WF-16 Plugin 与 MCP 接入

**前置条件：** Profile ready，操作者具有安装或连接权限，Capability Manifest 声明对应 Plugin/MCP 方法。

**正常路径：** 用户查看 Plugin 清单、权限与提供的 Skills/MCP/Apps，确认后安装到 Profile；或新增 MCP Server，完成配置、OAuth/凭据授权、reload 和连接测试。工具页展示 Server 状态、工具清单、授权范围和最近调用。Task 运行时由 Codex 原生工具系统调用，审批仍经过平台统一审批界面。

**异常与边界：** 未声明能力时操作禁用并显示所需 Rust bridge 版本；OAuth state 与回调绑定 Profile 和 Session；安装失败可重试且不能留下“已启用”假状态；Plugin 卸载前显示受影响能力；MCP elicitation 必须映射为可审计的结构化用户输入；平台不得实现第二套 Plugin 或 MCP Runtime。

**终态：** Plugin/MCP 的安装、版本、权限、连接和健康状态与 Codex 原生状态一致，所有敏感变更和授权均可审计。

## 10. 功能需求

优先级定义：P0 为 V1 发布阻断项，P1 为 V1 后首批增强，P2 为长期能力。

### 10.1 认证与会话

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| AUTH-001 | P0 | 邀请制账户创建与登录 | 未被邀请的用户不能加入组织 |
| AUTH-002 | P0 | 使用 HttpOnly、Secure、SameSite Cookie 管理会话 | 浏览器存储中不存在访问 Token |
| AUTH-003 | P0 | 支持退出、会话吊销和管理员禁用用户 | 吊销后现有 HTTP 与 WebSocket 会话均失效 |
| AUTH-004 | P0 | 登录、邀请和密码操作限流 | 暴力请求触发限流并记录安全事件 |
| AUTH-005 | P1 | OIDC/SSO | 可按组织启用并映射成员身份 |

### 10.2 组织、成员与权限

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| ORG-001 | P0 | 组织成员、邀请和角色管理 | 权限变更立即影响新请求 |
| ORG-002 | P0 | 所有业务记录携带组织边界 | 跨组织 ID 枚举返回不可见或拒绝 |
| ORG-003 | P0 | 项目级成员与角色覆盖 | 用户只能访问被授权项目 |
| ORG-004 | P0 | 关键权限服务端校验 | 前端隐藏不能替代服务端授权 |

### 10.3 项目与仓库

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| REPO-001 | P0 | 使用 Git URL 创建项目 | 可验证远端、凭据和默认分支 |
| REPO-002 | P0 | Git 凭据加密存储 | API、日志和事件不返回明文凭据 |
| REPO-003 | P0 | 仓库镜像和任务 Worktree 分离 | 两个 Run 不共享可变工作目录 |
| REPO-004 | P0 | 分支命名和冲突策略 | 重名、保护分支和 Push 冲突有明确反馈 |
| REPO-005 | P1 | GitHub App 或 OAuth 集成 | 可选择仓库并创建 PR |

### 10.4 Task、Thread 与 Run

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| RUN-001 | P0 | 创建 Task 并启动 Run | Task、Run、Workspace 和 Thread 关系可追踪 |
| RUN-002 | P0 | 运行状态机由服务端维护 | 刷新页面不会重置状态 |
| RUN-003 | P0 | 支持排队、开始、取消、失败、完成和中断 | 每个终态有原因、时间和操作者 |
| RUN-004 | P0 | Task 使用独立 Worktree 并绑定持久 Codex Profile | 不同 Task 不共享可写目录；同一 Profile 可跨 Task 复用身份、配置和记忆 |
| RUN-005 | P0 | 同项目并发限制和组织总并发限制 | 超限任务保持排队并显示原因 |
| RUN-006 | P0 | Run 失败后可基于原 Thread 创建新 Run | 历史 Run 保持只读并可审计 |
| RUN-007 | P1 | 定时任务和自动重试策略 | 仅对配置的失败类型生效 |

### 10.5 消息与实时事件

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| EVT-001 | P0 | 展示 Agent 消息、计划、工具、命令、文件变更和错误 | 事件按照服务端序号稳定排序 |
| EVT-002 | P0 | WebSocket 断线重连和游标补发 | 客户端可从最后确认序号恢复 |
| EVT-003 | P0 | 事件持久化与 Task 快照 | Server 重启后可重建可见状态 |
| EVT-004 | P0 | 增量消息合并幂等 | 重复投递不会产生重复文本 |
| EVT-005 | P0 | 大输出截断与 Artifact 化 | 页面不会因无限输出失去响应 |

### 10.6 审批与人工输入

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| APR-001 | P0 | 持久化命令、文件、权限和用户输入请求 | 刷新页面后请求仍可见 |
| APR-002 | P0 | 审批按角色和项目策略授权 | 无权限用户无法提交结果 |
| APR-003 | P0 | 审批支持同意、拒绝、过期和取消 | 每次决策记录操作者、时间和理由 |
| APR-004 | P0 | Worker 失联时冻结审批 | 不向已失效的进程请求 ID 发送响应 |
| APR-005 | P1 | 可配置低风险自动批准规则 | 规则变更有版本和审计记录 |

### 10.7 Git 与代码审查

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| GIT-001 | P0 | 展示文件列表、变更统计和 Diff | 二进制、大文件和删除文件有明确状态 |
| GIT-002 | P0 | Commit 前展示最终变更和测试摘要 | 用户确认后才执行 Commit |
| GIT-003 | P0 | Commit 和 Push 权限独立控制 | Reviewer 可审查但不能默认 Push |
| GIT-004 | P0 | Push 冲突和远端失败可恢复 | 不丢失本地 Commit 和 Workspace |
| GIT-005 | P1 | 创建 Pull Request | 保存 PR URL、编号和状态 |

### 10.8 协作、通知和审计

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| COL-001 | P0 | Task Control Lease | 同一时刻仅一个用户可发送控制指令 |
| COL-002 | P0 | 在线成员看到运行与租约变化 | 状态变化在目标延迟内同步 |
| COL-003 | P0 | 任务评论和成员提及 | 评论独立于 Codex 消息保存 |
| NOT-001 | P0 | 站内通知待审批、完成和失败 | 可读状态跨设备同步 |
| AUD-001 | P0 | 记录登录、权限、审批、执行、密钥和 Git 操作 | 管理员可按用户、项目、时间筛选 |
| NOT-002 | P1 | 邮件或 Web Push 通知 | 用户可配置通知偏好 |

### 10.9 平台管理

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| ADM-001 | P0 | Runner 心跳、版本和容量状态 | 不健康 Runner 不接收新任务 |
| ADM-002 | P0 | 查看队列、运行、失败和资源占用 | 可定位 Task、Run 和 Runner |
| ADM-003 | P0 | 组织和项目并发限制 | 变更后对新调度立即生效 |
| ADM-004 | P0 | 安全停止和强制终止 Run | 进程树和 Workspace 清理可追踪 |
| ADM-005 | P1 | 用量统计和导出 | 按用户、项目和时间聚合 |

### 10.10 全局导航与搜索

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| NAV-001 | P0 | 权限感知的主导航 | 无权限模块不显示且直接访问被服务端拒绝 |
| NAV-002 | P0 | 页面面包屑和资源标题 | 用户始终能确认当前组织、项目和 Task |
| NAV-003 | P0 | 全局命令面板 | 可搜索项目、Task 和成员并支持键盘导航 |
| NAV-004 | P0 | 返回时恢复列表上下文 | 筛选、分页、滚动和选中项按会话恢复 |
| NAV-005 | P0 | 全局连接与维护状态 | 离线、重连和维护期间不允许误提交写操作 |

### 10.11 工作台

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| DSH-001 | P0 | “需要我处理”队列 | 聚合待审批、待输入、租约请求和失败 Run |
| DSH-002 | P0 | 我的运行中与最近 Task | 状态与服务端一致，支持快速恢复上下文 |
| DSH-003 | P0 | 最近项目和创建 Task 主入口 | 一次点击进入项目或创建流程 |
| DSH-004 | P0 | 按优先级组织内容 | 待处理事项优先于统计和历史信息 |
| DSH-005 | P1 | 个人效率摘要 | 指标可解释且不使用虚构评分 |

### 10.12 项目页面

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| PRJ-001 | P0 | 项目概览 | 展示仓库、默认分支、活跃 Run、失败和容量 |
| PRJ-002 | P0 | 项目任务表 | 支持状态、成员、分支、标签和更新时间筛选 |
| PRJ-003 | P0 | 项目创建向导 | Git 验证、策略配置和镜像进度分步可恢复 |
| PRJ-004 | P0 | 项目成员管理 | 角色变化有确认、即时反馈和审计 |
| PRJ-005 | P0 | 执行与审批策略设置 | 保存前显示影响范围并进行服务端校验 |
| PRJ-006 | P0 | 项目危险操作区 | 禁用、归档、删除和凭据替换需要独立确认 |

### 10.13 Task 工作区

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| TSK-001 | P0 | 三区域任务工作区 | 导航、活动和检查器区域尺寸稳定且可调整 |
| TSK-002 | P0 | Task Header | 显示标题、项目、分支、Run 状态、租约和主操作 |
| TSK-003 | P0 | Run 历史切换 | 历史 Run 只读，当前 Run 明确标识 |
| TSK-004 | P0 | 活动流分类与折叠 | Agent、计划、工具、审批、评论和系统事件可区分 |
| TSK-005 | P0 | 新活动提示 | 阅读历史时不强制滚动，用户可一键回到底部 |
| TSK-006 | P0 | 右侧检查器 Tabs | Changes、Files、Logs、Run Details 状态独立保存 |
| TSK-007 | P0 | Run 终态摘要 | 展示结果、耗时、测试、变更和建议下一步 |
| TSK-008 | P0 | 危险操作确认 | Stop、Discard、Commit、Push 不共用模糊确认文案 |

### 10.14 Composer 与控制

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| CMP-001 | P0 | 多行输入、附件和发送 | 输入法、粘贴、拖放和键盘发送行为一致 |
| CMP-002 | P0 | Control Lease 状态 | 无租约时输入区明确只读并提供请求控制入口 |
| CMP-003 | P0 | Run 状态感知 | queued、running、waiting 和终态提供正确操作 |
| CMP-004 | P0 | 草稿按用户和 Task 隔离 | 租约转移不会把草稿暴露给其他成员 |
| CMP-005 | P0 | Steer 与 Queue 语义明确 | 当前行为在发送按钮菜单中可见且可撤销排队消息 |
| CMP-006 | P0 | 附件安全与进度 | 上传失败可重试，未完成上传不能提交 |

### 10.15 审批界面

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| APV-001 | P0 | 审批上下文完整 | 展示来源 Task、Run、命令/文件、目录、风险和原因 |
| APV-002 | P0 | 风险等级视觉语义 | 风险不只通过颜色表达，高风险需要更强确认 |
| APV-003 | P0 | 同意和拒绝不对称防误触 | 危险同意与普通按钮在位置和确认上有区分 |
| APV-004 | P0 | 过期与失联状态 | 不可处理时按钮禁用并解释原因 |
| APV-005 | P0 | 批量审批边界 | V1 不允许批量处理高风险审批 |
| APV-006 | P1 | 规则建议 | 仅在成功审批后建议规则，不默认启用 |

### 10.16 Diff、文件与日志

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| DIF-001 | P0 | 文件变更树 | 支持路径折叠、状态、统计、搜索和筛选 |
| DIF-002 | P0 | Unified 与 Split Diff | 用户选择按个人偏好保存，不改变数据 |
| DIF-003 | P0 | 大 Diff 渐进加载 | 不阻塞任务活动流，加载范围可见 |
| DIF-004 | P0 | 文件查看只读边界 | V1 不提供通用代码编辑器，不产生未审计写入 |
| DIF-005 | P0 | 日志虚拟滚动与搜索 | 大日志保持可交互并支持下载授权 |
| DIF-006 | P0 | 测试结果与 Artifact | 来源、时间、退出状态和截断信息明确 |

### 10.17 UI 状态与反馈

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| UX-001 | P0 | 每页定义 Loading、Empty、Error、Forbidden 和 Offline | 不使用空白页面或无限 Spinner |
| UX-002 | P0 | 写操作使用乐观或保守策略清单 | 高风险操作必须等待服务端确认 |
| UX-003 | P0 | Toast 只用于短暂反馈 | 持续故障和待处理事项使用页面内状态 |
| UX-004 | P0 | 表单错误定位到字段 | 服务端错误保留输入并提供可执行建议 |
| UX-005 | P0 | 破坏性操作使用资源名确认 | 不使用统一“确定吗”文案 |
| UX-006 | P0 | 时间与状态一致 | 同时提供相对时间和可访问的绝对时间 |
| UX-007 | P0 | 用户文案面向结果 | 不向普通用户暴露 RPC 方法名和内部异常栈 |
| UX-008 | P0 | UI 偏好与业务状态分离 | 清除浏览器存储不改变任务事实 |

### 10.18 响应式与可访问性

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| RSP-001 | P0 | 宽屏三栏、窄屏双栏、手机单栏 | 无水平页面溢出和不可访问操作 |
| RSP-002 | P0 | 手机底部任务 Tabs | Activity、Changes、Approvals 和 Details 可切换 |
| RSP-003 | P0 | 面板调整不导致内容跳动 | 使用稳定最小/最大尺寸并持久化个人偏好 |
| RSP-004 | P0 | 触控目标满足最小尺寸 | 手机主要操作无需精细点击 |
| RSP-005 | P0 | 虚拟键盘不遮挡 Composer | 输入和发送在移动浏览器可完成 |
| A11Y-001 | P0 | 键盘完成核心工作流 | 登录、创建 Task、审批、Diff 和发送可操作 |
| A11Y-002 | P0 | 焦点管理 | Drawer、Dialog、Tab 和新事件焦点行为可预测 |
| A11Y-003 | P0 | 语义和辅助技术标签 | 图标按钮、状态、表格和实时区域有可读名称 |
| A11Y-004 | P0 | 对比度和非颜色表达 | 状态、风险和 Diff 同时使用文本或图标 |
| A11Y-005 | P0 | 减少动态效果 | 尊重 `prefers-reduced-motion` 且不影响理解 |

### 10.19 Codex Profile

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| PRO-001 | P0 | 一个用户默认绑定一个持久个人 Profile | 跨登录、跨 Task 和 Host 重启后使用同一受保护 Codex Home |
| PRO-002 | P0 | Profile 级身份与凭据隔离 | 用户无法读取、枚举或借用其他 Profile 的身份、配置和 Secret |
| PRO-003 | P0 | 一个 Profile 同时最多一个主 app-server | 并发启动幂等，冲突实例被阻止并产生审计 |
| PRO-004 | P0 | 一个 Profile 注册多个授权 Workspace | Workspace 移除不删除 Profile、Thread 或全局配置 |
| PRO-005 | P0 | Profile 健康与生命周期管理 | 可查看版本、进程、认证、能力、活动 Turn 和最近错误 |
| PRO-006 | P0 | 安全重启与 Thread 恢复 | 使用同一 Codex Home 重启，已知 Thread 可重新读取或继续 |
| PRO-007 | P0 | Profile 导出、退出身份、记忆重置和删除分离 | 每项说明影响范围、二次确认并独立审计 |
| PRO-008 | P1 | 管理员服务 Profile | 仅用于明确的自动化任务，不作为成员共享个人 Profile |

### 10.20 原生 Agents 与多 Agent

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| AGT-001 | P0 | 读取与管理 Codex 原生 Agent | 配置写入 Profile 原生位置并可由 app-server 重新读取 |
| AGT-002 | P0 | 配置 multi-agent 开关、线程数和深度 | UI 值与 Codex 原生配置及实际限制一致 |
| AGT-003 | P0 | 创建 Task 时选择可用原生 Agent | Run 保存 Agent 标识和不可变配置摘要，不复制 Agent Runtime |
| AGT-004 | P0 | 展示父子 Thread 与协同事件 | 派生、委派、等待、返回、失败和汇总关系可追踪 |
| AGT-005 | P0 | 子 Agent 状态恢复 | 刷新或重连后关系来自 Codex Thread/Event，不由浏览器猜测 |
| AGT-006 | P0 | 保持 Codex 调度主权 | 平台代码中不存在自定义 planner、subagent scheduler 或委派规则引擎 |
| AGT-007 | P0 | 运行中限制与失败可解释 | 原生最大深度、最大线程和能力错误原样分类并面向用户解释 |
| AGT-008 | P1 | Agent 配置变更记录 | 可确认某个 Run 使用的 Agent 版本和变更来源 |

### 10.21 Skills Studio

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| SKL-001 | P0 | 列出个人与项目 Skills | 显示作用域、来源、版本、状态和覆盖关系 |
| SKL-002 | P0 | 创建符合规范的 Skill | 可维护 `SKILL.md` 及受控的 `scripts`、`references`、`assets` |
| SKL-003 | P0 | 使用 Codex 原生能力校验 Skill | 结构、元数据、路径和内容错误可定位，不自建解释器 |
| SKL-004 | P0 | 隔离测试 Skill | 测试 Task 显示发现、触发、工具调用、结果和失败原因 |
| SKL-005 | P0 | 发布个人 Skill 到 Profile | 发布后新 Turn 可发现，失败不会产生已发布假状态 |
| SKL-006 | P0 | 发布团队 Skill 到 `.agents/skills` | 变更进入项目 Worktree、Diff、Commit 和审查流程 |
| SKL-007 | P0 | Skill 版本、停用与回滚 | 已用版本可追踪，回滚结果由 Codex 重新加载验证 |
| SKL-008 | P0 | Skill 文件安全边界 | 防路径穿越、符号链接逃逸、超限文件和未授权二进制写入 |
| SKL-009 | P1 | 由 Codex 辅助生成 Skill | 生成内容仍经过相同编辑、校验、测试和发布门禁 |
| SKL-010 | P1 | Skill 使用与失败观测 | 按版本记录匿名化调用结果，不采集代码和 Prompt 正文 |

### 10.22 Plugin Manager

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| PLG-001 | P0 | 读取已安装 Plugin 与 Manifest | 显示版本、来源、能力、权限和健康状态 |
| PLG-002 | P0 | 安装经允许来源的 Plugin | 用户确认权限后由 Codex 原生 Plugin bridge 完成安装 |
| PLG-003 | P0 | 升级、停用、启用和卸载 | 结果与原生状态一致，并显示受影响 Skills/MCP/Apps |
| PLG-004 | P0 | Plugin 来源与完整性策略 | V1 仅允许管理员配置的仓库、包或签名策略 |
| PLG-005 | P0 | Plugin 权限变更确认 | 新增权限不能静默随升级获得 |
| PLG-006 | P0 | Plugin 操作审计 | 记录操作者、Profile、来源、版本、权限摘要和结果 |
| PLG-007 | P0 | 缺少 bridge 时能力降级 | 模块显示 unavailable 和所需版本，不启用替代 Runtime |
| PLG-008 | P1 | 安装前兼容性检查 | 提前发现 Codex 版本、平台策略和依赖冲突 |

### 10.23 MCP 与 Tools

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| MCP-001 | P0 | 列出 Profile 可用 MCP Server 与工具 | 状态、来源、工具数和授权范围与 app-server 一致 |
| MCP-002 | P0 | 新增和编辑 MCP 配置 | Secret 使用服务端引用，不返回浏览器或写入普通日志 |
| MCP-003 | P0 | reload 与连接测试 | 配置变更后可确认实际加载状态和分类错误 |
| MCP-004 | P0 | OAuth 授权闭环 | state、callback、Session 和 Profile 强绑定并防重放 |
| MCP-005 | P0 | elicitation 映射为结构化输入 | 刷新后仍可处理，过期和并发响应规则明确 |
| MCP-006 | P0 | 工具调用权限与审批 | 复用 Codex 原生调用和平台审批，不绕过项目策略 |
| MCP-007 | P0 | 工具状态与最近调用观测 | 不记录 Secret、Prompt 正文或敏感工具结果 |
| MCP-008 | P0 | Server 停用与删除影响提示 | 有活动 Turn 时按协议拒绝、延迟或中断并明确反馈 |
| MCP-009 | P0 | 缺少 bridge 时能力降级 | 不以 Web 自建 MCP Client 冒充 Codex 原生能力 |
| MCP-010 | P1 | 项目级 MCP 策略 | 可限制 Profile 工具在指定项目中的可用范围 |

### 10.24 记忆与上下文连续性

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| MEM-001 | P0 | Codex 是 Thread、压缩和记忆的事实来源 | 平台 DB 不生成、合并或替换 Codex 记忆 |
| MEM-002 | P0 | 同一 Profile 跨 Task 保持原生记忆 | app-server 重启和新 Worktree 不更换 Codex Home |
| MEM-003 | P0 | Thread/Turn 映射持久化 | 平台 Task 可恢复到准确 Codex Thread，映射有唯一约束 |
| MEM-004 | P0 | 展示上下文压缩与记忆状态事件 | 只呈现 Codex 提供的状态、时间和错误，不伪造内容 |
| MEM-005 | P0 | 记忆导出与重置治理 | 影响范围、活动 Thread、不可逆风险和审计明确 |
| MEM-006 | P0 | 跨 Profile 记忆隔离测试 | 两个用户使用相似任务时不能互见 Thread、摘要或记忆 |
| MEM-007 | P1 | 记忆诊断 | 可查看容量、最近 consolidation 状态和恢复建议，不暴露内部敏感正文 |

### 10.25 Capability 与双项目契约

| 编号 | 优先级 | 需求 | 验收标准 |
| --- | --- | --- | --- |
| CAP-001 | P0 | app-server 启动时返回版本化 Capability Manifest | 每项能力含稳定 ID、版本、方法/事件支持和限制 |
| CAP-002 | P0 | Web 按能力启用模块与操作 | 未支持功能禁用并解释原因，不在运行时盲试 RPC |
| CAP-003 | P0 | 固定兼容版本矩阵 | Web 发布物声明支持的 Codex Rust 构建和协议版本范围 |
| CAP-004 | P0 | 请求、事件和错误 Schema 可生成 | 两个项目从同一版本化合同生成类型或通过合同测试 |
| CAP-005 | P0 | Rust bridge 保持 Codex 原生语义 | Web 不增加会改变 Thread、Agent、Skill、Plugin、MCP 或记忆含义的协议 |
| CAP-006 | P0 | 跨项目兼容 Fixtures | CI 对 Profile、Thread、Agent、Skill、Plugin、MCP、审批和恢复回放测试 |
| CAP-007 | P0 | 不兼容升级可阻止调度 | Profile 标记 incompatible，现有数据可导出且不被破坏性迁移 |
| CAP-008 | P0 | 缺失能力有明确责任归属 | 进入 Codex Rust 项目补 bridge，不在 Web 项目复制 Runtime |

## 11. 权限矩阵

| 操作 | Owner | Project Admin | Developer | Reviewer | Viewer |
| --- | --- | --- | --- | --- | --- |
| 管理组织成员 | 是 | 否 | 否 | 否 | 否 |
| 管理项目与凭据 | 是 | 是 | 否 | 否 | 否 |
| 创建和运行任务 | 是 | 是 | 是 | 否 | 否 |
| 获取 Control Lease | 是 | 是 | 是 | 否 | 否 |
| 审批普通命令 | 是 | 是 | 是，可受策略限制 | 是，可受策略限制 | 否 |
| 审批高风险权限 | 是 | 是 | 否 | 否 | 否 |
| 查看 Diff 和日志 | 是 | 是 | 是 | 是 | 是 |
| Commit | 是 | 是 | 是 | 否 | 否 |
| Push | 是 | 是 | 是，可受策略限制 | 否 | 否 |
| 管理个人 Codex Profile | 是，仅本人 | 是，仅本人 | 是，仅本人 | 是，仅本人 | 是，仅本人 |
| 重置个人 Profile 记忆 | 是，仅本人 | 是，仅本人 | 是，仅本人 | 是，仅本人 | 是，仅本人 |
| 管理原生 Agent | 是 | 是 | 是，仅授权作用域 | 否 | 否 |
| 发布项目 Skill | 是 | 是 | 是，可受分支策略限制 | 否 | 否 |
| 安装或升级 Plugin | 是 | 是，可受组织策略限制 | 否 | 否 | 否 |
| 配置 MCP 与 OAuth | 是 | 是，可受组织策略限制 | 是，仅个人且受策略限制 | 否 | 否 |
| 查看完整审计 | 是 | 项目范围 | 自身相关 | 项目只读摘要 | 否 |

所有权限均由服务端基于 Organization、Project、Task 和资源所有权计算。平台管理员不自动获得业务仓库内容访问权；紧急访问必须单独审计。

## 12. 状态模型

### 12.1 Run 状态

`queued -> provisioning -> running -> waiting_approval | waiting_input -> running -> completed`

异常终态包括 `failed`、`cancelled` 和 `interrupted`。终态不可回退；继续工作必须创建新 Run。`interrupted` 表示 Worker 或 app-server 失联，平台不能证明原 Turn 正常完成。

### 12.2 Approval 状态

`pending -> approved | denied | expired | cancelled`

审批决策必须幂等。同一请求只接受第一个合法终态，后续提交返回当前结果。

### 12.3 Workspace 状态

`provisioning -> ready -> in_use -> releasing -> released`

任一步骤失败进入 `error`，由清理任务重试；清理失败必须告警，不能静默遗留凭据或进程。

## 13. 核心数据对象

V1 至少包含：`users`、`sessions`、`organizations`、`memberships`、`projects`、`project_memberships`、`repositories`、`codex_profiles`、`profile_bindings`、`capability_snapshots`、`tasks`、`task_members`、`codex_thread_mappings`、`runs`、`run_events`、`run_snapshots`、`workspaces`、`workspace_registrations`、`runner_leases`、`control_leases`、`approvals`、`comments`、`skill_publications`、`integration_operations`、`artifacts`、`notifications`、`secrets`、`audit_events` 和 `runner_nodes`。

平台表只保存多用户治理所需的映射、权限、快照、操作记录和索引：`codex_profiles` 不保存可由 Codex Home 还原的原生配置正文；`codex_thread_mappings` 不复制 Thread/Turn 内容作为新事实来源；`skill_publications` 记录发布流程，个人 Skill 正文归 Profile，项目 Skill 正文归 Git；Plugin 与 MCP 的实际安装状态以 Codex 返回为准。

所有租户资源必须携带 `organization_id`；数据库索引和唯一约束必须包含租户边界。所有可重试写操作使用幂等键。

## 14. 非功能需求

### 14.1 性能与容量目标

- V1 容量目标：100 个在线 Web 会话、20 个并发 Run、单组织 200 个项目。
- 普通 API 在稳定负载下 p95 小于 300 ms，不包含 Git、Codex 和大文件操作。
- app-server 事件到在线浏览器的 p95 延迟小于 1 秒。
- 断线后补发 10,000 条事件在 5 秒内完成，超限时使用快照加增量。
- 任务详情首次可交互时间目标小于 2.5 秒，不加载完整大日志。

### 14.2 可用性与恢复目标

- V1 服务可用性目标为月度 99.5%，计划维护除外。
- PostgreSQL 数据 RPO 不超过 5 分钟，RTO 不超过 60 分钟。
- Server 重启后恢复排队任务；运行中任务根据 Runner 租约重新确认或标记中断。
- Workspace 清理和 Artifact 保留由可重试后台任务执行。

### 14.3 安全要求

- TLS、CSP、CSRF 防护、严格 CORS、请求限流和安全响应头。
- 密钥静态加密、传输加密、按需解密和日志脱敏。
- Runner 使用非 root 身份；公共网络部署使用 rootless 容器、资源限制和受控出网。
- 禁止挂载 Docker Socket、宿主用户目录或跨项目可写路径。
- 防止路径穿越、符号链接越界、命令参数注入和 Git URL SSRF。
- Token、Cookie、Git 凭据和 Codex 凭据不得进入 URL、前端持久化或普通日志。
- 高风险操作和安全配置变更写入不可由普通用户修改的审计记录。

### 14.4 兼容性与可访问性

- 支持 Chrome、Edge、Firefox、Safari 最近两个主版本。
- 核心登录、任务、审批和 Diff 流程符合 WCAG 2.1 AA 目标。
- 键盘可完成主要工作流；状态不只依赖颜色表达。
- 桌面浏览器提供完整能力，移动浏览器保证观察、回复、审批和通知可用。

## 15. 数据保留与隐私

- Task、Run、审批和审计默认保留 180 天，可由管理员配置。
- 原始命令输出默认保留 30 天；超大输出转 Artifact 并按策略清理。
- Workspace 在 Run 完成后进入保留期，默认 24 小时，之后安全清理。
- 删除用户不会删除其历史审计主体；显示为停用用户。
- 删除项目需要二次确认和延迟执行，期间允许管理员撤销。

## 16. 产品指标

| 指标 | 定义 | V1 观察目标 |
| --- | --- | --- |
| 首次成功时间 | 新组织从初始化到首个成功 Run 的时间 | 中位数小于 30 分钟 |
| Run 成功率 | `completed / 已进入 running 的 Run` | 持续监控，不以模型质量单独定责 |
| 平台故障率 | 因 Server、Runner 或协议错误失败的 Run 比例 | 小于 2% |
| 排队延迟 | queued 到 provisioning | p95 小于 60 秒，有容量时小于 5 秒 |
| 审批响应时间 | pending 到终态 | 按项目和风险类型观察 |
| 重连恢复率 | 断线后无人工刷新恢复的会话比例 | 大于 99% |
| 交付转化 | 完成后产生 Commit 或 Push 的 Task 比例 | 观察产品价值，不设硬门槛 |

## 17. 发布范围

### 17.1 Alpha

内部单管理员、单项目验证，完成仓库导入、Task、Run、消息、审批、Diff 和取消闭环。允许使用进程级隔离，不对外开放。

### 17.2 Beta

启用邀请制多用户、RBAC、Control Lease、审计、持久化事件和 rootless 容器。完成桌面与 Web 功能并行验证。

### 17.3 V1 GA

Web 成为唯一客户端；删除 Tauri。完成备份恢复、容量测试、安全测试、升级回滚和运维手册。

## 18. 风险与约束

| 风险 | 影响 | 产品处理 |
| --- | --- | --- |
| app-server 协议变化 | 运行或事件解析失败 | 锁定 CLI 版本、协议适配层、契约测试和升级门禁 |
| Profile 身份或记忆串用 | 用量、隐私、权限和审计无法归属 | 成员默认独立持久 Profile、凭据引用隔离、Thread/Memory 跨 Profile 拒绝测试 |
| Agent 执行恶意命令 | 数据泄露或宿主破坏 | 容器隔离、审批、出网限制和最小凭据 |
| Worker 崩溃 | Run 卡住或审批失效 | 租约、心跳、明确 interrupted 状态和清理任务 |
| Git 并发冲突 | Push 失败或覆盖变更 | 每 Task/Run 独立 Worktree/分支、保护分支策略和非强制 Push |
| Codex Rust bridge 延期或不完整 | Codex Studio 或记忆治理无法发布 | Capability 门控、双项目里程碑和版本化交付；不在 Web 侧实现替代 Runtime |
| Profile app-server 单实例容量不足 | 同一用户并发 Task 排队或变慢 | 以 Codex 原生线程限制做容量测试；仅在 Rust Runtime 明确支持后设计分片 |
| 前端迁移范围过大 | 交付延期 | 先交付纵向闭环，按功能切换 Transport，不进行一次性重写 |

## 19. 产品决策与待决策项

### 19.1 已锁定决策

1. V1 是 Codex Web Harness，不是独立 Agent Runtime；Codex 是 Thread、Turn、多 Agent、记忆、Agents、Skills、Plugins 和 MCP 的运行时事实来源。
2. 一个成员默认拥有一个持久个人 Codex Profile；Profile 之间强隔离，同一 Profile 跨 Task 复用 Codex Home、身份、配置和记忆。
3. 每个 Run 在所属 Task 边界内使用独立 Git Worktree；一个 Profile 的主 app-server 可管理多个已授权 Workspace 和 Thread，而不是每个 Run 启动一套 Codex Home。
4. 团队共享 Skill 进入项目 `.agents/skills` 和 Git 审查；V1 不通过多人共用可变个人 Profile 实现共享。
5. 缺少的 app-server bridge 在并行 Codex Rust 改造项目中补齐；Web 项目不得以自建运行时临时替代。
6. Web 是唯一用户客户端，不要求 Tauri、桌面程序、本地 CLI、本地桥接进程或浏览器扩展。

### 19.2 阶段 0 待关闭事项

1. V1 首个生产部署仅支持 Linux Worker，还是同时支持 Windows Worker。
2. Git 首发支持通用 HTTPS/SSH，还是优先 GitHub App。
3. 审批策略采用预置风险等级，还是允许管理员自定义规则表达式。
4. Workspace 默认保留时长及组织可配置范围。
5. Codex Rust 改造项目与 Web Harness 的首个冻结协议版本、发布节奏和兼容窗口。

## 20. 页面与功能模块设计

### 20.1 登录与邀请页

**页面目标：** 用最少信息完成安全登录或受邀加入，不展示产品营销内容。

**页面结构：** 居中单列表单，顶部为产品名称，下方依次为组织上下文、身份字段、主操作和辅助链接。邀请模式必须显示邀请组织、角色、邀请人和过期时间。登录模式不显示不可用的注册入口。

**主操作：** 登录、接受邀请。辅助操作包括忘记凭据、返回登录和联系管理员。

**状态设计：** 提交期间锁定重复提交但保留取消导航；认证失败不暴露账户是否存在；邀请过期展示重新申请路径；会话失效后返回登录并保留原安全目标 URL。

### 20.2 初始化向导

**页面目标：** 帮助 Owner 在一次可恢复流程中获得可运行的首个项目。

**步骤：** 组织信息、Codex Profile、能力检查、Git Credential、首个仓库、成员邀请、最终检查。

**交互规则：** 左侧显示步骤和完成状态，右侧显示当前表单；每一步服务端保存草稿；只有当前步骤验证通过才允许继续；允许返回修改；最终激活前显示配置摘要和安全提醒。

**失败处理：** Codex 和 Git 测试输出提供用户可理解的类别、时间和重试，不展示完整环境变量或命令行。暂时无法完成时可退出，工作台显示继续初始化入口。

### 20.3 工作台

**页面目标：** 用户进入后在十秒内判断“现在需要我处理什么”。

**内容顺序：** 需要我处理、我的运行中、最近 Task、最近项目、平台提示。需要我处理使用紧凑列表，不能被统计卡片挤到首屏以下。

**列表字段：** Task 标题、项目、原因、等待时长、风险、当前负责人和直接操作。待审批可进入详情但不在工作台直接执行高风险同意。

**空状态：** 新组织 Owner 引导创建项目；普通成员无项目时说明等待分配；有项目无 Task 时提供创建入口；筛选无结果时提供清除筛选。

**不包含：** 虚构生产力分数、大面积趋势图、装饰性欢迎 Hero 和与当前任务无关的系统指标。

### 20.4 项目列表

**页面目标：** 快速定位项目并识别不可运行或需要维护的项目。

**页面结构：** 标题与创建按钮、搜索与筛选工具栏、项目表格。默认按最近活动排序；用户可切换名称、更新时间和活跃 Run 排序。

**表格字段：** 项目名称、仓库、默认分支、活跃 Run、最近活动、健康状态和用户角色。项目健康异常必须给出可操作原因。

**交互：** 单击行进入项目；行尾菜单仅放低频操作；删除不放在列表主操作；筛选和排序写入 URL Query 以便分享和返回恢复。

### 20.5 项目概览与任务列表

**项目概览：** 顶部显示项目名称、仓库、默认分支和健康状态；主体使用全宽区块展示活跃 Task、需要处理的失败、最近交付和成员，不使用嵌套卡片。

**任务列表：** 使用表格或紧凑列表，字段包括状态、标题、当前 Run、Owner、分支、最后活动和变更统计。支持多条件筛选、保存个人筛选和分页/游标加载。

**项目设置：** 使用左侧设置导航，分为 General、Repository、Execution、Approvals、Members、Retention 和 Danger Zone。保存操作按设置域独立提交，避免一个超长表单。

### 20.6 创建 Task

创建入口使用 Drawer 或独立页面，不使用多层 Modal。第一屏只展示任务目标、基线分支、Codex 原生 Agent 和附件；当前 Profile、模型、推理等级、执行参数和环境选择位于“高级设置”。不可用 Agent 或能力保留可解释的禁用状态。

任务目标输入提供足够高度但不伪装成聊天窗口。分支选择显示默认、最近和搜索结果。附件区域显示上传进度、类型、大小和删除操作。提交按钮文案为“创建并运行”，旁边明确当前审批策略。

验证错误就地显示；服务端失败保留全部输入；成功后直接进入 Task 工作区并显示 queued 状态。

### 20.7 审批中心

**页面目标：** 让授权用户安全、高效地处理跨项目等待事项。

**页面结构：** 左侧筛选或顶部工具栏，中间审批列表，右侧详情检查器。默认只显示待处理，支持项目、风险、请求类型、发起时间和状态筛选。

**列表字段：** 风险、请求摘要、项目、Task、等待时长和请求类型。高风险项目不能只显示红点，必须显示风险文本。

**详情：** 展示完整命令或文件范围、工作目录、触发原因、Agent 说明、关联 Diff、审批策略和历史相似决策。长命令使用代码块和横向滚动，不自动折行改变语义。

**操作：** 拒绝、批准一次、按允许范围批准；高风险同意需要二次确认。V1 不提供高风险批量批准。

### 20.8 Codex Studio

Codex Studio 使用统一二级导航，包含 Profiles、Agents、Skills、Plugins、MCP & Tools。顶部固定显示当前 Profile、Codex/app-server 版本、健康状态和 Capability Manifest 摘要；切换 Profile 时所有列表与操作立即重新授权，禁止混用缓存。

**Profiles：** 概览显示身份状态、进程、Codex Home 标识、能力版本、活动 Turn、已注册 Workspace、记忆/压缩最近状态和错误。主要操作是连接身份、健康检查和安全重启；导出、退出身份、重置记忆和删除位于独立 Danger Zone，分别说明数据影响并要求资源名确认。

**Native Agents：** 使用紧凑表格展示名称、说明、模型、作用域、更新时间和可用状态。编辑页直接映射 Codex 原生 Agent 字段与 `multi_agent_enabled`、`max_threads`、`max_depth`，保存前显示配置 Diff；运行引用保留快照。界面不得创建平台私有的 Agent 模板语义。

**Skills Studio：** 左侧按 Personal、Project 和 Plugin Provided 分组，中间为文件树与编辑器，右侧为 Metadata、Validation、Test Runs 和 Versions。创建流程支持模板和 Codex 辅助生成；校验通过后才能测试，测试通过后才能标记 verified。个人发布写入 Profile，项目发布产生 `.agents/skills` Git Diff；发布、停用和回滚都有状态、版本及审计。

**Plugin Manager：** 列表显示来源、版本、提供能力、权限和健康状态。详情先展示 Manifest、权限变化与影响，再提供安装、升级、停用和卸载。高风险权限使用逐项确认；操作期间保持 pending 状态，只有 Codex 原生查询确认后才显示 installed 或 enabled。

**MCP & Tools：** Server 表格显示连接状态、来源、传输方式、工具数、OAuth 状态和最近错误。配置 Drawer 使用 Secret 引用而非明文回填；测试视图展示 DNS、进程、协议、认证和工具发现分步结果。OAuth 与 elicitation 使用专用流程；工具调用历史只保存非敏感元数据。

**能力降级：** 每个模块和操作由 Capability Manifest 驱动。bridge 缺失时保留页面上下文并显示 unsupported、所需 Codex Rust 版本和管理员处理入口，不显示无效按钮，也不切换到平台自建实现。

### 20.9 团队与权限

成员页使用表格展示姓名、邮箱、组织角色、项目数、状态和最后登录。邀请、修改角色、禁用和移除使用不同操作；Owner 降权和最后一个 Owner 删除必须被阻止。

角色说明在修改时就近显示具体能力，不要求用户另查文档。项目权限覆盖必须明确标识来源，避免用户无法判断权限来自组织还是项目。

### 20.10 平台管理

Runner 页面是运维工具，使用表格和详情 Drawer，不使用营销式状态卡片。字段包括状态、版本、最后心跳、容量、运行数、队列、磁盘和最近错误。

队列页展示 Run、组织、项目、排队原因、等待时间、优先级和目标 Runner。管理员可以暂停调度、排空 Runner 和终止异常 Run，但不能修改业务 Prompt。

审计页支持结构化筛选、事件详情和导出。敏感字段始终脱敏；导出属于受审计操作。

## 21. Task 工作区 UI 规格

Task 工作区是产品的核心界面，优先级高于工作台和管理页。其设计目标是在一块屏幕中同时支持“跟随 Agent、做出决策、检查代码”，但任何时刻只突出一个主操作。

### 21.1 桌面布局

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Global Nav │ Project / Task title │ Run status │ Lease │ Stop · More       │
├────────────┼───────────────────────────────┬────────────────────────────────┤
│ Project &  │ Activity                     │ Inspector                       │
│ Task       │ Agent messages               │ Changes | Files | Logs | Run    │
│ navigation │ Plans and tool events         │                                │
│            │ Approvals and comments        │ Selected detail                │
│            │                               │                                │
│            ├───────────────────────────────┤                                │
│            │ Composer / read-only lease bar│                                │
└────────────┴───────────────────────────────┴────────────────────────────────┘
```

- 全局导航展开宽度目标为 224 px，收起为 56 px。
- Task 导航区目标宽度 260-340 px，可收起；中间活动区最小 480 px。
- Inspector 目标宽度 360-520 px，可调整和收起；调整时内容区不得跳动或覆盖。
- 顶部 Task Header 高度稳定，不因状态文本或成员头像改变高度。
- Composer 最小高度适合一行输入，最大高度约 220 px，超出后内部滚动。

具体尺寸在视觉概念确认后固化为 Design Token；上述范围属于布局约束而非自由缩放。

### 21.2 Task Header

Header 左侧显示返回、Task 标题、项目和分支；中部显示 Run 状态、排队/运行时间和事件连接状态；右侧显示 Control Lease、停止和更多菜单。

- 标题允许两行以内，超长时省略并可 Tooltip 查看完整内容。
- running 使用动态但克制的状态指示，不使用持续大面积动画。
- waiting_approval 和 waiting_input 必须成为 Header 的最高优先状态。
- Stop 只在可停止状态显示；完成后替换为 Continue。
- Commit 和 Push 属于 Changes 工作流，不长期占据 Header。

### 21.3 Task 导航区

顶部为当前项目和创建 Task 快捷操作；主体按 Running、Needs attention、Recent 分组展示 Task。每行显示状态图标、标题、未读、Owner 和最后活动。

导航区不展示完整消息预览，避免噪声。搜索结果保留项目边界。隐藏/归档 Task 不在默认列表中，但可通过筛选访问。

### 21.4 活动流

活动流使用开放时间线，不把每条消息包装成独立浮动卡片。用户消息和 Agent 最终消息具有清晰文本层级；计划、工具、命令、Diff 更新和系统事件使用紧凑可折叠行。

- 推理内容按产品能力和权限展示，不与最终回答使用相同视觉权重。
- 命令事件展示命令、目录、状态、耗时和截断信息；输出默认折叠。
- 文件变更事件显示文件数和统计，点击后打开 Inspector Changes。
- 审批请求在活动流中使用固定位置的任务内面板，决策后折叠为结果摘要。
- 评论具有成员身份和时间，视觉上与 Agent 消息区分。
- 连续增量合并到同一事件节点，不因 Token 流产生布局抖动。

### 21.5 Composer

Composer 顶部可显示当前发送模式、附件和等待状态；主体为多行输入；右侧使用发送图标按钮，运行中提供 Queue/Steer 菜单。停止使用独立图标按钮，不复用发送按钮形状。

- 无 Control Lease 时替换为只读提示条和“请求控制”操作。
- waiting_input 时输入框切换为请求所需的结构化输入，不允许同时发送普通 follow-up。
- queued 或 provisioning 时允许编辑初始后续消息，但明确标识为排队消息。
- 草稿只属于当前用户和 Task；清除浏览器数据可以丢失草稿，但不影响已发送内容。
- 上传附件、粘贴图片和拖放文件采用统一队列和错误反馈。

### 21.6 Inspector

Inspector 使用 Tabs，而不是在同一页面堆叠多个面板：

| Tab | 主要内容 | 主操作 |
| --- | --- | --- |
| Changes | 文件树、统计、Diff、测试摘要 | 打开 Commit Drawer |
| Files | 只读项目文件树和内容 | 搜索、复制路径 |
| Logs | 命令和 Runner 日志 | 搜索、下载授权 Artifact |
| Run | Run、模型、用量、Runner、时间和故障 | 复制诊断、继续或重试 |

Inspector 每个 Tab 独立保存选中项和滚动位置。收起后出现有未读变化的状态点，但不持续闪烁。

### 21.7 Commit Drawer

Drawer 展示目标分支、文件范围、变更统计、测试状态和 Commit Message。默认包含当前 Run 全部未提交变更；V1 不提供任意部分文件编辑，但可以在产品确认后支持文件级选择。

Commit 成功后 Drawer 显示 Commit SHA 和 Push 操作。Push 再次显示远端、分支和保护策略。失败时保留 Drawer 状态和本地 Commit，不把错误隐藏在 Toast 中。

## 22. UI/UX 设计系统方向

### 22.1 视觉定位

产品属于高频研发操作工具，视觉方向为安静、清晰、专业和高信息密度。避免营销页面、巨大标题、Bento 卡片墙、装饰渐变、发光效果和拟物终端。视觉焦点来自真实任务状态、代码 Diff 和审批，而非插画。

### 22.2 色彩

- Light 基础背景使用纯白或中性浅灰，不能默认改成奶油色、米色或暖棕色。
- Dark 使用中性黑灰，避免整套界面由深蓝/Slate 单色主导。
- Accent 使用清晰蓝色承载选中和主要操作；不能同时用于成功状态。
- Success、Warning、Danger 和 Info 使用不同语义色，并始终搭配文字或图标。
- Diff Added 与 Removed 需要兼顾浅色、深色和色觉差异，不依赖整行高饱和底色。
- 不使用装饰性渐变；唯一允许的渐变是解决滚动边缘或内容淡出等功能需求。

最终色值由高保真视觉概念和现有 Design Token 审计共同确定，在实现前形成 Color Lock。

### 22.3 字体与密度

- 使用系统 UI 字体或现有产品字体；代码、命令和路径使用等宽字体。
- 页面标题建议 18-22 px，面板标题 14-16 px，正文 13-15 px，辅助文字不小于 12 px。
- 字号不随 Viewport 宽度连续缩放；Letter Spacing 为 0。
- 表格和工具栏保持紧凑但不拥挤，行高按可点击性和扫描效率确定。
- 长路径、SHA、分支和无空格文本必须允许中间省略或安全换行，不撑破容器。

### 22.4 容器与层级

- 页面区块使用开放布局、分隔线和背景带；Card 只用于重复对象、审批和明确的独立工具。
- 禁止 Card 内嵌套 Card；Modal、Drawer 和 Popover 不再套装饰性外卡片。
- Card 和 Panel 圆角不超过 8 px，除非现有 Design System 已有更严格规则。
- 阴影仅用于浮层、拖拽和需要表达层级的临时元素；常驻面板使用 Border。
- 工具栏、Tabs、列表和表格尺寸稳定，Hover、Badge 和状态变化不能造成布局位移。

### 22.5 图标与控件

- 使用现有 Lucide 图标集；图标按钮提供 Tooltip 和可访问名称。
- 保存、发送、停止、刷新、展开、复制、下载和搜索使用熟悉图标，不制造带文字的装饰胶囊。
- 模式选择使用 Segmented Control 或 Menu；二元设置使用 Switch；数字使用 Input/Stepper；风险规则使用明确选择控件。
- Primary Button 每个页面区域最多一个；危险操作不使用 Primary Accent 色。
- Disabled 状态必须说明原因，尤其是权限、租约和 Run 状态造成的禁用。

### 22.6 动效

- 动效用于展开、Tab 切换、新事件到达和状态过渡，时长短且可被减少动态设置关闭。
- 实时增量不逐字符制造强烈闪烁；内容流式增长但保持阅读位置稳定。
- 排队和运行状态可以使用轻量脉冲或旋转图标，不使用大面积循环背景动画。
- Toast、Drawer 和 Modal 退出后焦点返回触发元素。

### 22.7 文案

- 使用“任务、运行、审批、项目、变更”等用户术语，不显示 `thread/start`、RPC ID 或 Rust 错误栈。
- 错误文案包含发生了什么、是否影响数据、用户能做什么和诊断编号。
- 危险确认明确资源名和后果，例如“终止 Run #123 并停止当前命令”，不用“确定继续吗”。
- 系统不宣称 Agent 已成功，除非收到服务端终态并具备对应证据。

## 23. 响应式与跨设备设计

### 23.1 断点行为

| 宽度 | 布局 | 行为 |
| --- | --- | --- |
| `>= 1280 px` | 三栏任务工作区 | Task 导航、Activity、Inspector 同时可见 |
| `800-1279 px` | 双栏 | 全局导航收起，Inspector 作为可固定 Drawer |
| `< 800 px` | 单栏 | Activity、Changes、Approvals、Details 使用底部 Tabs |

断点按内容是否可用确定，不按设备名称判断。布局切换不得丢失 Composer 草稿、选中文件或活动流位置。

### 23.2 手机端能力边界

手机端 P0 支持登录、工作台、任务观察、消息发送、结构化输入、审批、评论、停止 Run、查看变更摘要和通知。复杂 Split Diff、批量文件审查、项目高级设置和 Runner 运维可以提供简化视图或提示使用更宽屏幕，但不能显示不可操作的完整控件。

### 23.3 虚拟键盘与安全区域

Composer 聚焦时使用 Visual Viewport 计算可用高度，底部操作避开系统安全区域。键盘出现不能把 Task Header 和发送按钮同时挤出视口；活动流保留当前阅读锚点。

## 24. 状态、反馈与异常界面

### 24.1 页面状态矩阵

| 状态 | 表现 | 禁止做法 |
| --- | --- | --- |
| Initial Loading | 页面骨架保持最终结构 | 全屏无限 Spinner |
| Empty | 说明原因并给唯一下一步 | 用装饰插画取代操作 |
| No Results | 保留筛选并提供清除 | 与真正无数据共用文案 |
| Forbidden | 说明缺少访问权和申请路径 | 泄露资源细节 |
| Offline | 持续状态条、只读已缓存内容 | 继续接受不可提交写操作 |
| Reconnecting | 显示重试和最后同步时间 | 把 Run 标记失败 |
| Partial Error | 保留成功区域并局部重试 | 整页清空 |
| Fatal Error | 提供诊断 ID、返回和重试 | 暴露内部堆栈 |

### 24.2 写操作反馈

- 低风险且易回滚的偏好设置可以乐观更新。
- 成员权限、审批、Run 控制、Commit、Push、凭据和删除必须等待服务端确认。
- 请求超时不等于失败；界面先查询幂等结果再允许重试。
- Toast 只报告短暂成功；失败、冲突和待处理状态留在对应页面直到解决。
- 同一按钮提交期间保持宽度和位置稳定，使用内部进度图标。

### 24.3 实时状态

连接状态分为 live、reconnecting、offline 和 stale。stale 表示页面有缓存但无法确认最新状态。用户在 stale 状态不能执行审批、Stop、Commit 或 Push。

Run 状态只能由服务端事件或查询更新。客户端可以显示“请求已发送”，但不能提前显示“已停止”“已提交”或“已推送”。

## 25. 产品与安全边界

### 25.1 浏览器边界

- 浏览器不能访问用户电脑上的 Codex CLI、仓库、终端或文件系统。
- 不要求安装桌面客户端、本地 Agent、浏览器扩展或 Tauri Runtime。
- 浏览器只通过同源平台 API 和认证 WebSocket 访问业务能力。
- `localStorage` 仅保存主题、面板尺寸和无敏感性的草稿/偏好。

### 25.2 Codex 边界

- Codex CLI/app-server 位于服务端 Codex Host/Runner，浏览器依赖平台 API，不直接依赖或启动 CLI。
- Codex 原生 Thread、Turn、多 Agent、上下文压缩、记忆、Agents、Skills、Plugins 和 MCP 是运行时事实来源；平台不得复制其核心算法与状态机。
- app-server 原始协议和请求 ID 不构成浏览器公开合同；版本化 Capability Manifest、Schema、错误分类和兼容矩阵构成 Web 与 Codex Rust 项目的内部集成合同。
- UI 只展示当前 Codex 构建实际返回且平台适配层支持的模型、Agents、Skills、Plugins、MCP、记忆状态和能力。
- Rust bridge 尚未提供的能力必须显示 unavailable；不得以 Web Job、自建解释器或另一套 MCP/Plugin Runtime 替代。
- API Key 身份、ChatGPT/Codex 登录身份和平台登录身份不能默认等价。
- CLI 升级必须通过契约和 Smoke Test，不能由用户在 UI 中任意升级生产 Runner。

### 25.3 Workspace 与 Git 边界

- 普通用户不能注册服务器任意路径；项目来源是受控 Git Repository。
- 每个 Run 使用独立可写 Workspace；仓库镜像只读或由平台专用流程更新。
- V1 Files 为只读浏览器，不提供通用在线编辑器。
- V1 Logs 为只读输出，不提供任意交互 Shell。若未来引入 Shell，必须作为独立高风险功能评审。
- 平台不自动 Commit、Force Push、合并或删除远端分支。

### 25.4 协作边界

- Control Lease 只控制 Agent 消息和 Run 控制，不自动赋予审批、Commit 或 Push 权限。
- Reviewer 可以审查和按策略审批，但不能借审批获得写代码权限。
- Platform Admin 管理基础设施，不默认获得仓库内容和业务 Prompt 的浏览权限。
- 评论与 Agent 消息是不同对象，删除评论不改写 Agent 历史。

### 25.5 数据与隐私边界

- Prompt、代码、日志和 Diff 属于组织数据，默认不进入产品分析事件。
- 产品分析只记录资源 ID、状态、耗时、错误分类和交互事件。
- 凭据不进入浏览器、URL、普通日志、分析和错误上报。
- 导出、下载 Artifact 和查看敏感审计详情均需授权并记录审计。

### 25.6 并行 Codex Rust 改造项目边界

V1 由两个同步项目共同交付。两者以版本化协议发布物集成，不通过复制源码逻辑或共享数据库耦合。

| 能力域 | Codex Rust 改造项目负责 | CodexMonitor Web 项目负责 |
| --- | --- | --- |
| Thread/Turn/事件 | 原生生命周期、恢复、事件与错误语义 | Task/Run 映射、授权转发、展示、游标补发和审计 |
| 多 Agent | 原生 Agent 配置、派生、调度、深度/线程限制和协同事件 | 配置 UI、父子轨迹展示和运行快照；不做调度 |
| 记忆 | 压缩、consolidation、持久化、读取/导出/重置 bridge | Profile 生命周期、治理入口、隔离、恢复验证和审计 |
| Skills | list/read/write/validate/reload/test bridge 与原生发现语义 | Studio、作用域授权、项目 Git 发布、测试编排和版本展示 |
| Plugins | install/list/read/update/enable/disable/uninstall bridge、Manifest 与权限语义 | Manager、来源策略、确认流程、状态展示和审计 |
| MCP/Tools | 配置、reload、OAuth、elicitation、状态、工具发现与调用协议 | Secret 托管、OAuth Web 回调、审批 UI、健康与调用元数据 |
| Capability Contract | Manifest、协议版本、Schema、Fixtures 和稳定错误码 | 能力门控、兼容矩阵、适配层、合同测试和升级门禁 |
| 多用户与 Git | 不负责平台账号、RBAC、组织、项目和 Worktree 治理 | 账号、RBAC、租户隔离、Codex Host、Worktree、Commit/Push |

Rust 项目每次交付必须包含可机器读取的 Capability Manifest、Schema/类型输入、兼容说明、迁移说明和回放 Fixtures。Web 项目只在相应合同通过 CI 后启用功能。任何一方修改已发布语义都必须提升协议版本并提供兼容或明确阻断策略。

## 26. 产品分析与设计验收

### 26.1 核心产品事件

| 事件 | 触发时机 | 允许属性 |
| --- | --- | --- |
| `onboarding_completed` | 组织初始化完成 | 耗时、失败步骤数，不含凭据 |
| `project_created` | 项目 ready | Provider 类型、耗时、结果 |
| `task_created` | Task 创建成功 | 项目 ID、原生 Agent ID、Profile ID、附件数量 |
| `profile_state_changed` | Profile 生命周期变化 | 状态、Codex 版本、能力版本、原因分类 |
| `skill_publication_changed` | Skill 发布状态变化 | 作用域、版本、结果，不含正文 |
| `integration_operation_completed` | Plugin/MCP 操作完成 | 类型、版本、权限等级、结果，不含 Secret |
| `run_state_changed` | Run 状态改变 | 前后状态、原因分类、耗时 |
| `approval_resolved` | 审批终态 | 类型、风险、结果、等待时长 |
| `control_lease_changed` | 租约转移 | 原因、等待时长，不含草稿 |
| `diff_opened` | 用户首次查看 Changes | 文件数、变更行数区间 |
| `commit_completed` | Commit 成功 | 文件数、耗时，不含 Message |
| `push_completed` | Push 成功 | Provider、结果、耗时 |
| `realtime_recovered` | 断线补发完成 | 断线时长、补发数量、结果 |

### 26.2 视觉设计交付物

在前端实现前必须完成并评审以下高保真概念：

1. 工作台桌面态及“需要我处理”非空状态。
2. Task 工作区 running 桌面态。
3. Task 工作区 waiting_approval 和 completed + Diff 状态。
4. 项目创建向导和项目任务列表。
5. 审批中心高风险详情。
6. Task 工作区手机态，包括虚拟键盘和底部 Tabs。
7. Loading、Empty、Forbidden、Offline 和 Fatal Error 状态板。

概念确认后必须提取 Color Lock、字体、间距、面板尺寸、图标、组件变体、可见文案和响应式行为。实现不得在没有产品/设计变更记录的情况下新增首屏模块或改变容器模型。

### 26.3 页面验收矩阵

| 页面 | 必验状态 | 必验视口 | 必验路径 |
| --- | --- | --- | --- |
| 登录 | 默认、失败、过期邀请、限流 | 桌面、手机 | 登录、邀请加入、会话失效 |
| 工作台 | 有待办、无项目、无 Task、离线 | 桌面、手机 | 进入审批、恢复 Task、创建 Task |
| 项目 | ready、setup、failed、无权限 | 桌面、平板 | 创建、筛选、设置、危险操作 |
| Task | queued、running、waiting、completed、failed、interrupted | 桌面、平板、手机 | 发送、重连、审批、接管、停止、继续 |
| Changes | 无变更、大 Diff、二进制、Commit、Push 失败 | 桌面、平板 | 审查、Commit、Push、冲突恢复 |
| 审批中心 | 待处理、已决、过期、Runner 失联 | 桌面、手机 | 同意、拒绝、并发决策、查看 Task |
| Codex Profiles | ready、auth required、degraded、incompatible、stopped | 桌面、手机 | 连接、检查、重启、恢复、危险操作 |
| Native Agents | 默认、multi-agent、校验失败、能力缺失 | 桌面、平板 | 创建、编辑、保存 Diff、运行引用 |
| Skills Studio | 草稿、校验失败、测试中、已发布、冲突、回滚 | 桌面、平板 | 创建、编辑、校验、测试、发布、回滚 |
| Plugin/MCP | 未安装、授权中、可用、失败、权限变化、unsupported | 桌面、平板 | 安装、OAuth、测试、升级、停用、卸载 |
| 团队设置 | 邀请、角色变化、最后 Owner、禁用 | 桌面 | 邀请、改权、吊销会话 |
| Runner 管理 | healthy、draining、offline、version mismatch | 桌面 | 暂停、排空、终止、升级 |

每个页面验收同时检查可见文案、信息层级、容器模型、字体、图标、焦点、键盘、颜色语义、滚动、移动溢出和真实状态更新。功能测试不能替代视觉一致性测试。

## 27. V1 总体验收标准

V1 只有在以下条件全部满足时才能发布：

- 两名以上用户可同时登录并运行不同任务，权限与事件无串流。
- 每个 Task/Run 使用独立可写 Workspace；不同用户 Profile 的 Codex Home、身份、Thread 和记忆强隔离。
- 同一 Profile 使用持久 Codex Home 和唯一主 app-server；Host 重启后可恢复 Profile、已知 Thread 与原生记忆连续性。
- 页面刷新、网络断开和 Server 重启后，任务状态可从服务端和 Codex 原生 Thread 恢复。
- 原生多 Agent 的父子 Thread、协同事件和限制在 Web 中完整可见，平台中不存在自建调度器。
- 用户可完成 Skill 创建、校验、隔离测试、个人/项目发布和回滚，并在实际 Task 中验证生效。
- 用户可完成 Plugin 安装治理与 MCP 配置、OAuth、elicitation、健康测试，状态与 Codex 原生查询一致。
- Web 与 Codex Rust 构建通过 Capability、Schema、Fixture 和升级兼容测试；缺失能力只降级，不启用替代 Runtime。
- 审批、控制权、Commit、Push 和安全配置均有审计记录。
- Worker 崩溃后 Run 在租约超时内进入明确状态，不永久停留在 running。
- 安全测试无法通过 URL Token、路径穿越、跨项目 ID 或日志获取凭据。
- 完整流程不要求安装 Tauri、桌面程序、浏览器扩展或本地桥接进程。
- 生产构建、CI、文档和发布物中不再包含桌面客户端。

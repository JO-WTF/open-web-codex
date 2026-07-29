# ADR-007：Workspace Dataset Release

- 状态：Accepted
- 日期：2026-07-29

## 背景

用户需要从 Web 端把业务数据加入一个已授权 Workspace，随后让 Runtime 发现的
MCP Tool 处理这些数据。现有 Workspace API 只能列出和读取已有文件，既没有上传
入口，也没有稳定的数据集身份、版本、文件角色、内容摘要和发布终态。让浏览器直接
指定服务器路径，或让 Skill/MCP 遍历目录猜测可用数据，都会破坏 Workspace
授权边界、增加模型上下文，并使一次分析无法复现。

数据文件属于 Workspace 内容；用户、授权、发布身份、审计和生命周期属于 Platform；
数据格式和业务有效性属于消费它的 MCP package。Codex Runtime 继续拥有 Tool、
Skill、MCP 的发现和执行，Platform 不解析仓网等领域数据，也不实现第二套 Tool
目录。

## 决定

引入一等的 `WorkspaceDatasetRelease`：

- Browser 只提交 Workspace ID、数据集稳定 ID、版本、显示信息、幂等键，以及每个
  上传文件的逻辑名称、用途和媒体类型；不得提交服务器本地路径；
- Platform 在 PostgreSQL 中持久化组织、Workspace、Owner、幂等键、不可变版本、
  发布状态、文件摘要和安全诊断。接受发布后，状态必须明确到达 `published` 或
  `failed`；
- Workspace/Git Runtime 在当前授权执行根下原子发布
  `datasets/<dataset-id>/<version>/`，其中 `release.json` 由服务端生成，
  用户文件位于 `files/`。临时目录、符号链接、文件数量、单文件大小和总大小均受
  有界校验；
- 同一 Workspace 中的 `dataset-id + version` 不可原地覆盖。相同幂等键和相同内容
  返回同一 Release；不同内容显式冲突；
- PostgreSQL 记录是发布身份、状态和摘要的权威事实，Workspace 文件是数据内容的
  权威事实，`release.json` 是由前者生成并与文件摘要锁定的 Runtime 可读投影；
- Browser DTO 只公开逻辑名称、用途、媒体类型、大小和 SHA-256，不公开绝对路径或
  Runtime URI；
- MCP package 通过用户明确选择的 Dataset Release 读取 `release.json`，校验摘要
  后再以流式方式处理文件。它不得依赖显示名称、任意目录扫描或把原始大表放入模型
  上下文；
- Platform 只校验通用发布合同。仓网字段、行政区边界、报价关系等领域规则由仓网
  MCP 的类型化验证 Tool 拥有。

## 能力门禁与恢复

- 创建和读取 Release 都要求当前用户对目标 Workspace 具有相应授权；
- 上传端点有固定请求体、文件数、单文件和总字节上限，超限在写入前失败；
- 文件发布失败时 Platform 记录 `failed` 及安全错误码；数据库最终确认失败时删除
  精确的已写入 Release 目录，不能留下看似成功的孤儿发布；
- 服务重启后，`publishing` 记录不能被当作可用数据。恢复工作只可依据稳定 Release
  ID、幂等键和摘要继续或终止，不得猜测目录内容；
- Tool 可用性仍来自 Codex 类型化发现和精确 Capability Package 选择。数据发布成功
  不代表 MCP 已启用或健康。

## 被否决的方案

### 通用文件管理器直接写任意 Workspace 路径

这会把浏览器输入提升为服务器路径权限，无法表达不可变版本和完整发布终态，也容易
覆盖源码或其他用户内容。

### 把数据存入 Thread、Artifact 或模型上下文

Thread 不是 Workspace 存储，Artifact 是任务交付物而不是分析输入。原始大表进入
上下文会造成不可控的 token、恢复和授权问题。

### 由 Skill 或 MCP 遍历 Workspace 猜数据

目录和文件名不是稳定合同。扫描结果不具备版本、角色和摘要，无法复现，也会把无关
内容带入关键路径。

### Platform 解析并校验每种业务数据

这会把领域语义放到错误所有者中，并逐步形成 Platform 侧 Tool 系统。Platform 只
拥有通用发布合同，领域 MCP 拥有数据语义。

## 后果与验证

- Web 可以用非技术化流程发布有版本的数据，Runtime MCP 可以按精确 Release 消费；
- 数据变更必须发布新版本，占用空间由显式保留/删除策略管理，不使用静默覆盖；
- PostgreSQL 集成测试覆盖授权拒绝、幂等、版本冲突、发布失败和重启后的非终态；
- Git Runtime 测试覆盖路径穿越、符号链接、大小上限、原子写入和精确清理；
- Browser 测试覆盖上传元数据、进度/错误、成功摘要和刷新后的 Release 列表；
- 真实 E2E 必须从 Web 发布教程数据，再由真实 Runtime MCP 读取并返回有界结果。

如果未来官方 Codex Workspace Resource 或 Dataset API 同时提供授权、版本、摘要和
Runtime 发现合同，应以新 ADR 替代本决定，并迁移出 Platform 自有发布协议。

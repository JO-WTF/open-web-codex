# ADR-006：Supervisor Capability Package

- 状态：Accepted
- 日期：2026-07-28

## 背景

产品需要允许用户在 Web 端定义、校验和发布 Supervisor，并选择它可以编排的
Agent、所需 Runtime 能力以及必须交付的 Artifact。现有内置 Supervisor Policy、
Agent Definition 和 Runtime Role 指令分散在 Web Server 与具体工具包目录中，
开发者无法从一个发布边界判断完整依赖，工具实现目录也被误当成 Agent 发布目录。

Codex Runtime 仍然拥有 Thread、Agent 执行、工具发现和 MCP 生命周期。平台不能为
了统一目录而实现第二套 Runtime 发现或执行系统。

## 决定

引入一等的 Supervisor Capability Package：

- `capabilities/supervisors/<policy-id>/<version>/` 保存代码管理的内置
  Supervisor manifest、指令和 Artifact 交付合同；
- `capabilities/agents/<definition-id>/<version>/` 保存代码管理的内置
  Agent Definition 和生成 Runtime Role 所需的指令；
- 工具实现、MCP Server、Skill 和 Plugin 继续留在各自所属包中，Package 只通过
  稳定能力标识和版本引用它们；
- Web 创建的有界 Python Capability Package 同样是不可变发布对象。版本必须同时
  进入发布身份、内容摘要和 Workspace 目录；同一 `package-id + version` 不可
  原地覆盖；
- Runtime 启动只接收当前 Thread 或受治理 Agent 精确选择的 Capability Package
  Release。普通 Thread 不再通过扫描 Workspace 的 `tools/` 目录隐式启用全部
  Package，Agent 也不得按显示名称或目录猜测可用 MCP；
- Supervisor Definition 是可编辑对象，Revision 是校验快照，Release 是不可变、
  可运行的发布对象；用户创建的对象保存在 PostgreSQL，不写入代码目录；
- Agent Definition 同样使用 Definition、草稿 Revision 和不可变 Release；当前过渡
  阶段必须选择一个经过代码评审的 Agent capability template，只能收窄其 Artifact
  输入输出，不能由浏览器声明 Runtime Role、MCP、Tool 或 capability；
- 代码 Package 与 Web 草稿只是同一个规范化 authoring spec 的两个输入 Adapter。
  Agent 和 Supervisor 各自只有一个服务端语义编译器；代码 Package 中声明的所有
  派生 Runtime 字段必须与编译结果完全相等，否则 Package 无效；
- Agent 发布时由同一个编译器根据 `definitionId + version` 稳定生成 Runtime Role
  身份与配置，锁定完整 spec、模板内容哈希和发布内容哈希；数据库资源 UUID、来源
  类型和创建顺序不得影响 Runtime 身份；
- Supervisor 发布时由同一个编译器从精确 Agent Definition/Release 派生 Role、
  Runtime requirement、Artifact handoff 和并发约束，再锁定 Agent Release UUID、
  版本与内容哈希，不允许按名称或版本回退；
- Supervisor 指令分为平台 Owner 发布的不可变
  `SupervisorInstructionPolicy`、Supervisor 作者填写的 `customInstructions` 和编译器
  生成的结构化执行合同。Release 只引用精确平台策略版本和摘要，不复制或允许作者
  覆盖平台文本；
- 平台策略、生成合同和自定义指令由同一编译器确定性合成为最终
  `developerInstructions`，再通过官方 `thread/start` 字段进入 Runtime；不得为此
  修改 Codex app-server 协议；
- 发布时锁定 Artifact 合同、Runtime 能力要求和内容哈希；
- 每个有效 Definition/Release 还公开一个来源无关的 execution-semantics SHA-256。
  该摘要覆盖指令、Artifact 合同、能力、Runtime Role 配置、MCP 要求、Agent 选择、
  handoff 和并发限制，用于等价性校验，不替代内容哈希或发布身份；
- 启动 Run 前由平台执行类型化预检，确认 Release 依赖与 Codex Runtime
  类型化能力发现一致；能力缺失显式失败；
- Runtime Role 由 Profile Host 为本次受治理执行物化，Thread 和 Agent 执行仍由
  官方 Codex app-server/Runtime 完成。

Agent 与 Tool 是多对多能力关系，不采用文件系统父子目录表达授权。Package 负责
声明关系，平台授权和 Runtime 能力发现负责判定是否可用。

## 被否决的方案

### 在根目录建立通用 `agents/` 并把工具实现放在其子目录

这会把发布定义、实现所有权和授权错误地绑定为目录层级，也无法表达多个 Agent
复用同一 Tool、同一 Agent 使用多个 Tool 的关系。

### 将用户发布的 Agent/Supervisor 写入仓库或 Profile `CODEX_HOME`

仓库不是用户治理数据存储，Profile 文件也不能承载发布、版本、审计和授权合同。
Profile 只保存 Runtime 可消费的物化结果。

### 在 Web Platform 中实现独立 Agent/Tool 发现与调度

这会复制 Codex Runtime 所拥有的能力并形成第二套 Agent 系统，违反项目所有权
边界，也增加上游同步成本。

## 后果与验证

- 内置发布资源有统一入口，但实现包仍保持各自所有权；
- 用户 Agent 和 Supervisor 的 Definition/Revision/Release 使用组织作用域的
  PostgreSQL 持久化与类型化 Web API；
- 发布校验必须拒绝不存在或版本不匹配的 Agent、无效 Artifact 交付关系和未声明
  的 Runtime 能力，也必须拒绝自定义 Agent 扩大所选 capability template；
- Workspace 发布的 Capability Package 必须持久化组织、Workspace、版本、内容
  SHA-256、MCP/Tool/Skill 清单和发布终态；浏览器只选择稳定 Release ID，路径由
  Platform 与 Profile Host 在服务端解析；
- 代码与 Web 等价性测试必须比较完整规范化执行语义及其摘要；任一可执行字段变更
  都必须改变摘要。代码 Package 中手写的派生字段发生漂移时必须显式失败；
- Release 一经发布不得原地修改；变更必须创建新版本；
- 平台指令策略同样不可原地修改；只有平台级 Owner 可以发布新版本。组织
  Supervisor 作者只能选择已发布版本；
- 运行预检必须覆盖能力缺失、依赖缺失、内容哈希不一致和重启恢复；
- 当前 Agent 发布是原生 Runtime CRUD 之前的过渡实现；模板关系是安全准入合同，
  不是 Tool/Plugin 自定义能力或第二套 Runtime discovery；
- 当前单用户阶段仍保留 organization、owner、profile 和 workspace 作用域，不能
  依赖全局 singleton。

若未来 Codex 官方提供完整的可发布 Supervisor Package 合同，应以新的 ADR
替代本决定，并删除平台重复部分。

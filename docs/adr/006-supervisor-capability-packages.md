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
- Supervisor Definition 是可编辑对象，Revision 是校验快照，Release 是不可变、
  可运行的发布对象；用户创建的对象保存在 PostgreSQL，不写入代码目录；
- 发布时锁定 Agent Definition 版本、Artifact 合同、Runtime 能力要求和内容哈希；
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
- 用户 Definition/Revision/Release 需要新的持久化和类型化 Web API；
- 发布校验必须拒绝不存在或版本不匹配的 Agent、无效 Artifact 交付关系和未声明
  的 Runtime 能力；
- Release 一经发布不得原地修改；变更必须创建新版本；
- 运行预检必须覆盖能力缺失、依赖缺失、内容哈希不一致和重启恢复；
- 当前单用户阶段仍保留 organization、owner、profile 和 workspace 作用域，不能
  依赖全局 singleton。

若未来 Codex 官方提供完整的可发布 Supervisor Package 合同，应以新的 ADR
替代本决定，并删除平台重复部分。

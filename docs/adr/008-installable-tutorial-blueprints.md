# ADR-008：版本化可安装 Tutorial Blueprint

- 状态：部分被 [ADR-014](014-release-installation-runtime-discovery.md) 与
  [ADR-017](017-clean-copilot-platform-spine.md) 替代
- 日期：2026-07-30
- 相关阶段：M2 Enterprise Supervisor Copilot
- 相关文档：[产品设计](../product-design.md)、
  [M2 短期计划](../enterprise-supervisor-copilot-plan.md)、
  [Workspace Dataset Release](007-workspace-dataset-releases.md)

仍有效：教程包必须版本化、幂等、无隐藏 fallback，并复用正式 Catalog owner。
已替代：Workspace-scoped reconcile、手工锁定旧 package/hash 和把旧仓网资源链作为当前
新手入口。未来 Blueprint 必须由统一 Compiler 生成，并通过 Profile Installation 与
Runtime discovery；本文其余内容仅保存历史背景。

## 背景

当前教程要求新用户先理解 Dataset、capability package、Agent、Supervisor、Artifact
和语义版本，再手工发布多个相互依赖的 Release。各对象的合同本身合理，但第一次业务
结果出现得太晚，而且任一步不可达或版本漂移都会迫使用户接触内部 ID、服务器路径或
开发命令。

平台需要一个可重复的快速体验入口，同时必须保留以下边界：

- Platform 拥有产品资源发布、授权、幂等和持久状态；
- Codex Runtime 仍拥有 Thread、Turn、Agent 协调和 Tool 执行；
- 示例安装不能变成写死 Agent 顺序的第二个 Workflow engine；
- 手工发布和教程安装不能形成两套 Dataset/Agent/Supervisor 语义；
- 教程进度不能成为与权威 Release、Run 和 Artifact 冲突的第二个状态源。

## 决定

引入只读、版本化的 **Tutorial Blueprint**。Blueprint 是 Platform 管理的安装声明，
不是 Runtime 指令或可执行 Workflow。

每个 Blueprint 使用稳定 `id + revision` 身份，声明：

- 构建时打包并校验的 Dataset bundle 及内容哈希；
- 精确 capability package、Agent、Supervisor 和平台行为合同版本及内容哈希；
- 推荐 Prompt revision；
- 必需 Runtime/MCP 能力；
- 预期 Artifact schema 和确定性验收值。

首个 Blueprint 为 `indonesia-warehouse-network@1.4.0`。Platform 提供有界的 list、
read 和 Workspace-scoped reconcile 合同：

```text
GET  /api/tutorial-blueprints
GET  /api/tutorial-blueprints/{id}/{revision}
POST /api/workspaces/{workspace_id}/tutorial-blueprints/{id}/{revision}/reconcile
```

reconcile 必须：

1. 通过当前 Dataset、capability package、Agent 和 Supervisor 发布 owner service
   创建资源，不能复制路由逻辑或直接写数据库；
2. 使用调用方提供的幂等键；
3. 只引用构建时校验过的仓库资产，拒绝浏览器路径、服务器路径或任意上传位置；
4. 对相同身份与相同哈希复用现有不可变 Release；
5. 对相同身份但不同内容显式返回冲突，不自动猜测或升级版本；
6. 逐阶段返回 `created/reused/blocked/failed` 等类型化结果；
7. 部分失败时保留已经成功发布的不可变资源，重试按身份和哈希继续；
8. 从权威 Dataset、Release、readiness、Run 和 Artifact 推导安装与教程完成状态，不
   新增独立教程进度表。

Blueprint 只准备可运行资源。用户仍需查看平台 readiness 并明确启动 Task。Runtime
根据业务目标和当前证据自主选择 Agent；Blueprint 不规定 Agent 数量、顺序、Tool
次数或某次 E2E 轨迹。

## 否决方案

### 在 Web 中硬编码示例资源和步骤

这会让浏览器拥有 Release 语义并通过显示名称推测能力，也会使代码声明和教程内容形成
第二套合同。

### 用启动脚本、SQL dump 或预制数据库安装

这会绕过正式授权、发布校验、审计和空环境初始化合同，并把宿主环境细节暴露给新用户。

### 把 Blueprint 实现成 Skill、Plugin 或 Supervisor 指令

Skill/Plugin/Runtime 不拥有平台 Dataset、Release 和 Workspace 授权生命周期。让模型
执行安装也无法提供可靠幂等、冲突和部分失败语义。

### 单独持久化教程完成进度

独立进度会在资源被删除、版本改变或 Run 失败后继续显示“完成”。权威资源已经足以推导
每一步状态。

### 失败时自动发布补丁版本

这会隐藏内容冲突，使教程、用户资源和 E2E 无法再指向同一不可变身份。

## 后果

- 快速体验与手工 Builder 路径共享同一资源合同和发布实现；
- Blueprint revision 变化必须显式发布，旧 revision 不在运行时迁移或兼容；
- 安装不是数据库事务；UI 必须如实显示部分成功并允许幂等继续；
- Blueprint manifest 和打包资产进入构建/CI 校验，但不进入 `codex/`；
- 未来增加其他案例时复用同一通用合同，不能为领域名称增加 Platform 分支；
- 真实 `/web` 从空 Workspace 完成安装、readiness、启动、交付和刷新恢复前，能力基线
  不得把该 Blueprint 声明为可用。

## 验证

- manifest 引用的 Release、版本、哈希、Artifact 和确定性答案全部存在且匹配；
- 重复 reconcile 不产生重复 Release；
- 内容哈希冲突、缺少资产、发布中断和跨 Organization 请求显式失败；
- 部分成功后重试只补齐缺失阶段；
- 浏览器 DTO 不包含 Secret、宿主路径、Runtime Role 或内部 Resource URI；
- 真实 Runtime/browser E2E 证明 Blueprint 不固定 Agent 轨迹，只验证权限、Artifact、
  报告和终态不变量。

## 被替代

若未来由新的平台 Resource Bundle 合同统一承担版本化示例、模板和其他可安装资源，
必须由新 ADR 明确替代本决定，并保留上述 ownership、幂等、冲突和无第二状态源约束。

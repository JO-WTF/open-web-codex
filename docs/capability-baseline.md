# Capability Baseline

本文记录当前分支的能力事实和证据等级。源码、迁移、局部测试通过和真实 `/web` E2E 是不同证据，不能相互替代。当前文档不保存已删除实现的流程说明；历史决策放在 ADR 和 Git 历史中。

## 当前快照

观察日期：2026-08-08。

| 能力 | 当前事实 | 证据等级 |
| --- | --- | --- |
| Codex Runtime | 继续使用官方 `requestUserInput`、子 Agent 生命周期和 Thread/Turn/Item 语义；现有 Chat transport 保留补丁补充标准与 DeepSeek 缓存命中 usage 解析 | `codex-api` 165 项测试和 Patch Map |
| Web Platform | 持有 Profile、Workspace、Thread、Run、Approval、Artifact 和事件投影；浏览器只访问平台 DTO | Rust 编译与已有平台测试 |
| Workspace 数据 | Workspace SourceAsset 支持受控上传、列表和持久化；Data Agent 通过 `workspace_intake.py` 发现 CSV/JSON/XLSX | 平台路由和 Python intake 测试 |
| User Input | root/child Runtime 输入请求归属 root Run，Approval UUID 和 version 用于并发回答；secret 回答不写入数据库或日志 | DTO、Service、路由和前端状态检查 |
| Agent execution | execution projection 保存标题、结果摘要、等待轮次、等待输入和终态序列；连续 wait 在前端聚合 | Rust cargo check；完整浏览器恢复仍需 E2E |
| Supervisor Draft | 草稿使用整数 revision 和内容 hash；发布时由服务器事务分配 Release patch version | Rust cargo check；并发数据库矩阵仍需运行 |
| Network package | Data、Network、Supervisor 当前包为 `6.0.0`；Supervisor 只绑定两个子 Agent，国家和调用顺序不写死 | capability package、catalog 和 seed migration |
| MCP 注册 | 只有 `supply_chain_network` 一个入口；Data、Planner 和显式 Demo 工具由同一 Case 服务提供 | `.mcp.json`、launcher、精确 tool inventory 测试 |
| Planner data contract | Agent 只交换同一个 `case_id`；来源、映射、标准化数据、矩阵和结果保存在 Case 中，只有最终报告发布为 Artifact | SQLite Case schema、service 和全量 Python 测试 |
| Indonesia fixture | 50 个需求城市、11 个已有仓、580 条报价；点位经过 geoBoundaries ADM2 校验，需求量按人口公式生成 | `validation-report.json` 和 fixture tests |
| Solver | OR-Tools CP-SAT；固定已有仓默认开启；超时和依赖缺失返回显式状态 | solver tests；真实 MCP 调用仍需 smoke |

## 当前 MCP 工具

单一 `supply_chain_network` MCP 暴露：

```text
create_network_case
get_network_case_status
define_network_requirements
refresh_case_sources
inspect_case_sources
propose_case_mapping
apply_case_mapping
normalize_case_input
plan_route_matrix
build_haversine_route_matrix
validate_route_matrix
plan_cost_matrix
evaluate_network_baseline
evaluate_facility_scenario
solve_p_median
solve_service_constrained_location
publish_network_planning_report
create_demo_workspace_sources
archive_network_case
```

旧的 intake Resource、`planning-dataset.v2`、analysis snapshot、旧 Indonesia 和 Visualization 工具不在 MCP 注册表中。`create_demo_workspace_sources` 只有收到明确 Demo 意图并确认目标 Workspace 为空时才写入已验证 fixture；空 Workspace、缺文件或真实 Tool 失败都不会触发 Mock。

## 关键不变量

1. Case 是 Agent 间业务状态的唯一来源；Agent 只传 `case_id` 和有限摘要，原始表格、矩阵和完整工具参数不能进入普通消息。
2. Data Agent 负责“数据能否使用、字段如何映射”；Network Agent 负责“需要什么、怎么算、结果代表什么”。
3. 名称匹配结果必须是 `exact`、`normalized`、`ambiguous` 或 `missing`；歧义必须请求用户，不能靠显示文本猜测。
4. 球面路线为 `haversine × detour_coefficient`，时效为调整后距离除以平均速度；每天驾驶小时是单独业务参数。
5. 有 current assignment 才能标记 `actual_current`；没有时只能标记 `optimized_existing_footprint`。
6. 有报价时精确报价优先；缺报价必须要求显式成本规则，不能填零。
7. p-median 只对上传的候选集合成立；固定已有仓、可关闭已有仓和新增候选必须是类型化输入。
8. `completed`、`failed`、`interrupted` 是 execution 终态；终态后乱序 running/waiting 不能回写运行状态。
9. `waiting_for_input` 只表示未解决的用户输入；普通协作等待使用 `waiting`，二者不能混用。

## 未完成的发布门禁

- 当前 Case 的 source/mapping 生命周期与 Platform Data Intake 重复；必须迁移到唯一
  owner 后删除领域副本。
- `event_projection.rs` 当前识别 `network-case-tool-result.v1`；必须改为通用有界 Tool
  result，平台核心不得包含领域分支。
- Root 尚无 Platform 提供的 Task/Run scoped 只读 coordination Tool；当前 Case 查询不能
  视为通用多 Agent 协作能力。
- Tool、Skill、Agent、Supervisor、Copilot 尚未共享统一 Catalog/Release/Installation/
  Runtime discovery 生命周期，Web 创作平台仍未完成。
- 运行全量 `./scripts/test-web-rust.sh`，确认新迁移、审批、执行投影和 Draft 并发测试在真实数据库中通过。
- 运行 Web typecheck、lint、Vitest、build，并验证多个输入卡片、子 Agent 输入、wait 聚合和刷新恢复。
- 运行 Python/MCP 全量 pytest、ruff、stdio smoke，并用真实 MCP 调用验证 50 城市流程。
- 在 `CODEX_MODE=real` 的 4800 Web 页面创建 6.0 Supervisor Thread，明确使用教程数据，回答参数输入，核对两个 execution 卡片、终态、报告、地图和刷新恢复。
- E2E 需要同时覆盖无 current coverage 和 `current-coverage-extension` 两种标签，不能只验证模型最终文本。

在上述证据完成前，当前能力只能称为“实现并通过局部边界验证”，不能声称印尼仓网真实浏览器闭环已发布。

---
name: prepare-network-input
description: 从授权 Workspace 的 CSV、JSON 或 XLSX 文件中，准备当前仓网问题真正需要的数据，并将结果写入 Network Case。
---

# 准备仓网输入

本 Skill 由 Data Agent 使用。Network Agent 负责说明本次问题需要哪些数据；Data Agent 负责判断用户文件能否满足要求以及如何转换。双方通过同一个 `case_id` 协作，不在消息中传递完整表格、文件路径或 Resource URI。

## 基础数据

- 需求城市：`city_id`、`city_name`、`demand_quantity`。
- 已有仓库：`warehouse_id`、`warehouse_name`、`warehouse_type`、`city_id`、`city_name`。
- 按问题选用：候选仓、当前覆盖、路线报价、上下级仓关系、坐标和行政区。
- 只接受授权 Workspace 中的 `.csv`、`.json`、`.xlsx`。空 Workspace 就是“没有数据”，不得自动加载示例数据。

## 操作顺序

1. 接收 Supervisor 交付的 `case_id`，调用 `get_network_case_status` 了解当前缺口。
2. 调用 `refresh_case_sources` 建立来源清单，再用 `inspect_case_sources` 查看有限的字段、类型和样例。不得使用 `ls`、`cat` 或遍历主机目录代替工具。
3. 调用 `propose_case_mapping` 生成持久化映射候选。映射必须包含来源、目标字段和转换方式。
4. 只把工具返回的候选 ID 提交给 `apply_case_mapping`。存在歧义时使用官方 `request_user_input`，不得根据文件名或显示名称猜测。
5. 调用 `normalize_case_input`。工具会在服务端重新读取完整文件并写入 Case，Agent 不接触完整数据行。
6. 再次调用 `get_network_case_status`，只汇报就绪项、缺口和下一步。

## 失败处理

- 缺少需求或已有仓字段时，用业务语言说明用户需要补充什么。
- 缺少当前覆盖、候选仓或报价，只影响依赖它们的分析，不得阻断无关分析。
- 工具失败必须保留 `failed`、`unavailable` 或 `needs_input`，不得填零、伪造成功或切换到 Mock。
- 只有用户明确要求 mock、demo、示例或教程数据时，才能使用 Demo Skill。

## 交付

只向 Supervisor 返回 `case_id`、Case 状态、有限问题摘要和建议的下一步。数据行由 Network Case 持久化，不能复制进对话上下文。

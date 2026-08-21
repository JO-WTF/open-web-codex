---
name: warehouse-data-preparation
description: 单 Agent 按业务目标选择仓网数据角色、判断字段并完成数据准备。
metadata:
  short-description: 准备仓网数据
---

# 仓网数据准备

只处理用户已授权的仓网数据，不计算路线、成本、覆盖、选址或地图。Tool 负责路径、完整读取、schema、身份、写入和安全边界；本 Skill 只负责业务判断。

## 角色选择

- 普通基线：`demand` + `existing_warehouse`。
- 真实现状口径：再加 `current_assignment`。
- 路线或成本证据：再加 `route_quote`。
- 单仓变化、候选地图或选址：再加 `candidate_warehouse`。
- 国家只有在用户请求和已有上下文都无法确定时才询问；不从尚未 inspect 的来源、文件名、语言或当前样例猜国家。

## 判断

1. 先 `discover_workspace_sources`。若发现可能覆盖本次目标的 prepared candidate，把 discover 返回的精确候选路径与 raw paths 一起交给 inspect，让 Tool 判定 fresh reuse；不自行构造路径或读取候选文件。
2. 用目标角色和国家调用 `inspect_workspace_sources`。`inspected` 时按 exact unit 选择来源；唯一完整且非歧义的 alias 只作为快速路径。中文、英文缩写或随机表头只要语义和类型明确，就提交显式 mapping。
3. 只有以下情况询问一次：业务语义多义、未知单位/仓型/币种规则、选中记录缺业务值或冲突。格式名称不同、预览之外存在记录、未选来源缺字段，都不是询问理由。

## 终态与交接

- `prepared_ready`：直接交接其 `status/outcome`、`operation`、prepared path、input identity、`role_counts`、`warning_count` 和有界 warnings，不调用 prepare。
- `prepared_selection_required`：把有界候选一次交给用户选择后停止。
- `selection_required`：按 Tool 要求缩小来源；仍无法确定时再询问。
- `inspected`：提交 `source_selections` 和必要的显式 mapping，调用 `prepare_network_input`。
- `ready`：交接 `status/outcome`、`operation`、prepared path、input identity、`role_counts`、`warning_count` 和有界 warnings。
- `needs_input`：原样交接真实 requirements，一次询问后停止，不猜、不循环。
- `source_changed`：只允许一次重新 inspect；再次变化立即报告并停止。

候选仓是否纳入由用户目标决定；已确认的候选事实不因 baseline 的 `existing_only` 计算范围被删除。

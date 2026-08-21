# 印尼仓网教程快速开始

这条路径只做一次最小真实运行：在 Web 中使用明确标记的教程数据，选择当前 `warehouse-network-copilot`，让 Data Agent 和 Network Agent 协作完成球面距离时效分析。

## 1. 检查环境

- 服务运行在真实 Runtime 模式；不要用 shell `source` 代替 MCP 调用。
- Workspace 可写且已授权。
- 一个 logical `supply_chain` MCP provider 可见。
- 当前 Copilot 只由 Supervisor、Data Agent 和 Network Agent 组成；地图交付由声明的 Map Tool 完成，没有额外 Visualization Agent。

## 2. 准备文件

从 [base fixture](../../tools/warehouse-network-planner/examples/indonesia-network/base/) 上传：

```text
demand-cities.csv
existing-warehouses.csv
administrative-areas.json
```

本教程明确使用 mock 数据，因此也可以在空 Workspace 中发送：

```text
使用教程 mock 数据，把它们复制到当前 Workspace；如果无法复制就报告失败，不要回退到其他数据。
```

没有明确的 mock/demo/tutorial 意图时，不复制或自动加载任何教程 fixture；本包已没有独立 Demo MCP。

## 3. 创建并运行 Thread

在 Web 中选择 **Warehouse Network Copilot**，创建 Thread，发送：

```text
规划印度尼西亚的仓网。先检查我提供的需求城市和已有仓库，使用球面距离 × 绕路系数估算距离和时效，不计算成本，不调用导航接口。给出 6、12、18 小时需求覆盖率，并说明输入缺口和假设。
```

如果是从空 Workspace 复制 fixture，必须在同一条消息中写明“使用教程 mock 数据”。

## 4. 输入卡片和执行卡片

Network Agent 会要求绕路系数、平均速度和每日驾驶时长。用输入卡片的按钮或自定义输入回答，不要在对话框中编写 `user_note:` 等伪协议。

前端应看到：

- 一个 Data Agent execution 卡片；
- 一个 Network Agent execution 卡片；
- 等待用户输入时的输入队列；
- 等待协作时的单个 execution 状态和 wait 次数；
- 每个 Agent 一个完成/失败/中断终态。

连续 wait 不应刷屏。子 Agent 的输入请求也必须显示在 root Run 的队列里，回答一次后卡片消失；刷新页面后仍可恢复未解决请求。

## 5. 结果检查

成功运行时，Data/Network 链至少产生以下 typed 结果：

```text
inline workspace_source_profile.v2 + workspace_source_inspection.v2
outputs/warehouse-network/prepared/*.json（prepared_network_input.v2）
route_matrix.v3
network_baseline.v2
network_planning_report_markdown.v2（单独 baseline 评估）
```

前两项分别是本次检查握手和用户可见的 Workspace 准备文件，不是 Data MCP Resource；路线、基线等 Network 中间计算才使用 provider-owned ResourceRef。

`network_planning_report_markdown.v2` 是 built-in final Tool 创建的 Workspace Markdown 交付物；baseline 评估使用独立 baseline report bundle，前后比较使用 generic before/after bundle。没有 current coverage 时，报告必须使用 `optimized_existing_footprint` 标签。需要实际当前方案时，继续阅读第三篇并上传 `current-coverage.csv`。

## 6. 出错时怎么判断

- 缺少字段：补文件或确认映射，不要让 Supervisor 猜。
- 缺少参数：回答输入卡片，不要重发同一条任务。
- MCP 调用失败：查看具体 Tool 的 typed error，不通过 `source`、shell 命令或目录遍历调用。
- OR-Tools unavailable：保持 unavailable，不切换旧求解器。
- 页面没有 Agent 卡片：检查 Run execution projection 和事件流，不把模型上下文全文当作日志。

完整案例按以下顺序继续：

1. [数据和球面时效](indonesia-network-01-data.md)
2. [两级仓网成本](indonesia-network-02-service-baseline.md)
3. [当前覆盖和场景](indonesia-network-03-two-level-cost.md)
4. [候选仓、选址和地图](indonesia-network-04-optimization-map.md)

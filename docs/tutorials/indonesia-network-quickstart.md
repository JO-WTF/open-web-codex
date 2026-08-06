# 10 分钟运行印尼仓网示例

本篇只让你获得第一次可验证结果，不要求手工创建 Dataset、Agent 或 Supervisor。完成后
你会看到一份确定性生成的仓网决策报告、八个有来源的 Resource，以及可恢复的
`report.v1` 报告卡片和 `map.v3` 地图卡片。

人工设置预计 5–10 分钟；模型和 Tool 的执行时间另计，也可能产生所选 Provider 的模型
费用。

## 开始条件

确认生产 `/web` 同时满足：

- 侧栏有 **Learn**；
- 示例列表有 `Indonesia warehouse-network decision · 1.5.0`；
- Workspace 已选择；
- Provider 和模型可用。

如果示例或入口缺失，停止并把它当作产品能力阻塞。不要运行仓库脚本、上传未知文件、
修改隐藏配置，或向对话发送 Workspace/Dataset/Runtime 内部 ID。

## 1. 设置准备好的示例

1. 打开 **Learn**。
2. 选择 **Indonesia warehouse-network decision · 1.5.0**。
3. 选择目标 Workspace。
4. 查看将创建或复用的 Dataset、capability package、Agent 和 Supervisor Release。
5. 点击 **Set up example**。

这个操作 reconcile 的是精确 Blueprint
`indonesia-warehouse-network@1.5.0`。相同身份和哈希会被复用；内容冲突或部分失败会
明确指出阶段。系统不会自动改版本，也不会回滚已经成功发布的不可变资源。

## 2. 通过 Readiness

安装完成后查看 **Readiness**：

| 检查 | 正确状态 |
| --- | --- |
| Runtime Profile | Ready |
| Provider and model | Ready |
| Execution definition | Ready |
| Workspace dependencies | Ready |
| Runtime capabilities | Ready |
| MCP servers | Ready |
| Map presentation | Ready，或仅因公开 Mapbox Token 缺失而 Degraded |

`Blocked` 项会提供 **Add data**、打开 Agent Studio、Provider Settings、MCP Status、
Maps Settings 或重试等明确动作。完成修复后回到同一启动草稿重新检查。

缺少 `map_utils` 会阻塞本篇地图任务；只有浏览器公开 Mapbox Token 缺失时可以继续。
后一种情况下分析、Resource 和地图 Artifact 仍应完成，卡片会明确提示不能绘制底图。

## 3. 启动任务

留在 **Learn** 中，确认 **Readiness** 没有阻塞项，然后点击 **Start task**。平台只有
在正式启动被接受后才显示新 Thread；重复点击不能创建重复 Task。

Thread 准备好后，Learn 会把受审的推荐 Prompt 放入对话输入框。先确认输入框中已经出现
完整任务，再点击 **Send** 开始运行。不要把 Workspace、Dataset、Runtime Role 或 MCP
内部 ID 补进 Prompt；这些绑定由 Blueprint 和 readiness 合同负责。

如果输入框原来已有自己的草稿，Learn 会保留它并明确提供替换动作，不会静默覆盖。
此时选择 **Replace draft with tutorial prompt**，检查替换后的内容，再点击 **Send**。

## 4. 处理审批

需要授权的 MCP 调用以审批卡显示。核对 Server、Tool、影响范围和本次任务后批准或拒绝，
不要在聊天框手工回复“批准”。

拒绝必须产生明确终态或可解释的部分结果；系统不能把拒绝变成成功，也不能改用隐藏
Tool。刷新页面后，未决或已决审批仍应保持同一平台审批身份。

## 5. 核对结果

完整任务应产生八个 Resource：

```text
indonesia_dataset_inspection.v1
indonesia_service_baseline.v1
indonesia_current_network_analysis.v1
indonesia_candidate_scenario.v1
indonesia_location_optimization.v1
indonesia_network_map.v1
geojson.v1
indonesia_decision_report.v1
```

以及一个 `report.v1` 报告交付 Artifact 和一个独立 `map.v3` 地图 Artifact。
确定性报告的关键值是：

| 指标 | 当前 | Jambi | 差值 |
| --- | ---: | ---: | ---: |
| 两日需求覆盖率 | 71.80% | 75.33% | +3.53 个百分点 |
| 年运输成本 IDR | 110,024,697,400 | 107,189,163,600 | -2,835,533,800 |
| 年度决策成本 IDR | 110,024,697,400 | 123,614,163,600 | +13,589,466,200 |

`indonesia_decision_report.v1` Resource 是报告内容、schema、digest、精确来源声明和
类型化 checks 的权威来源；配对的 `report.v1` 只负责把这个精确 Resource 交给浏览器
呈现。Network Agent 的 `REPORT_HANDOFF` 只包含报告 Resource 名和报告 Artifact ID，
随后独立出现一次 Tool 生成的报告 embed。模型正文不是报告来源，也不能复制、缩写或
重写报告内容。

要求地图时，Visualization Agent 的 `MAP_HANDOFF` 只保留两个输入 Resource 名和地图
Artifact ID；Tool 返回的 `structuredContent.embed.code` 必须作为独立指令出现一次，
不能再转义进该 JSON。浏览器以平台事件的类型化 `inlineArtifacts` 投影为
`report.v1` 和 `map.v3` 的权威实时与恢复视图，并分别校验引用的 Artifact ID。

最后刷新页面。报告、Agent Activity、审批、八个 Resource、`report.v1` 和 `map.v3`
卡片必须恢复为同一组记录。只有对话文字相似而 Tool、Artifact 或恢复证据缺失，不算
跑通。

## 下一步

- 想理解为什么这样拆分：阅读[案例与统一计算口径](supply-chain-agent-tutorial.md)。
- 想查看失败和恢复：阅读[审批与故障恢复](approvals-and-recovery.md)。
- 想自己构建：从[发布并核验大型规划数据](indonesia-network-01-data.md)开始。

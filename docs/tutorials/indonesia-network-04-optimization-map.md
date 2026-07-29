# 印尼仓网 4：有限候选优化与地图

本篇完成完整案例：

1. 核验数据；
2. 计算当前时效和两级运输成本；
3. 在 20 个已审核候选中选择一个新前置仓；
4. 要求 2 天需求覆盖率至少达到 74%；
5. 在满足目标的候选中最小化年度决策成本；
6. 创建当前方案与入选方案的地图卡片；
7. 由 Root Supervisor 汇总可审查报告。

比上一篇增加的是“完整有限候选优化”和独立 Visualization Agent。仍然不做连续空间
任意点求解，也不调用导航 API。

预计用时 30–45 分钟。先完成
[印尼仓网 3](indonesia-network-03-two-level-cost.md)。

## 已知答案

确定性优化会评估全部 20 个候选，并选择：

```text
CAN-JAMBI · Jambi Candidate
status = target_met
```

| 指标 | 当前 | Jambi | 增量 |
| --- | ---: | ---: | ---: |
| 1 天需求覆盖率 | 30.42% | 31.20% | +0.78 个百分点 |
| 2 天需求覆盖率 | 71.80% | 75.33% | +3.53 个百分点 |
| 3 天需求覆盖率 | 90.60% | 92.22% | +1.63 个百分点 |
| 年运输成本 IDR | 110,024,697,400 | 107,189,163,600 | -2,835,533,800 |
| 年度决策成本 IDR | 110,024,697,400 | 123,614,163,600 | +13,589,466,200 |

Jambi 承接 250,000 个年度需求单位，达到候选容量上限。它不是“印尼任意位置中的数学
最优点”，而是“当前 20 个已审核候选中，满足 74% 目标后年度决策成本最低的候选”。

## 1. 发布完整 Network Agent 版本

在 `tutorial-indonesia-network` 定义中点击 **New version**，创建 `3.0.0`。

Display name：

```text
Tutorial Indonesia Network Planner
```

Description：

```text
Evaluates the current network, the complete reviewed candidate set, and bounded map evidence.
```

**Responsibilities**：

```text
读取一个经过验证的印尼 Dataset inspection
核算当前两级仓网事实
在完整 20 候选集上执行确定性目标约束优化
为入选方案发布兼容的 scenario、map manifest 和 GeoJSON Resources
```

**Custom Agent instructions**：

```text
开始前必须收到完整的 indonesia_dataset_inspection.v1 resource_name 和结构化 data_ref。
缺失时停止，不列出 MCP Resources、不扫描 Workspace、不猜 URI。

按任务需要选择最小分析。需要当前两级事实时取得 current Resource；需要有限候选优化
时，把用户给出的 target_service_days、target_demand_coverage 和
opening_amortization_years 原样传给 optimize_indonesia_new_warehouse。必须接受 Tool
对完整 20 候选集的结果，不按城市偏好改选。

优化 Tool 返回入选 candidate scenario 后，只有用户要求地图时才调用
prepare_indonesia_network_map，并传入兼容的 current 和 selected scenario data_ref。
不得调用导航、读取客户点、自己生成候选或用模型重算成本。

每个 Domain Tool 在发布前原子校验 Resource。最终回答先输出全部原样
ARTIFACT_HANDOFFS，包括 current、optimization、selected scenario、network map 和
geojson 的 schema、resource_name、结构化 data_ref。明确区分有限候选最优与连续地理
最优，并报告目标状态、成本、容量、假设和限制。不要调用 create_map_card。
```

Artifact contracts：

```text
Input · indonesia_dataset_inspection.v1
Output · indonesia_current_network_analysis.v1
Output · indonesia_candidate_scenario.v1
Output · indonesia_location_optimization.v1
Output · indonesia_network_map.v1
Output · geojson.v1
```

依次点击：

```text
Save draft → Validate → Publish
```

## 2. 使用已有 Visualization Agent

本篇不需要再创建一个自定义地图 Agent。平台已经发布：

```text
Enterprise Visualization Agent · 1.1.0
```

它的权限边界是：

| 能力 | 是否允许 |
| --- | --- |
| 读取精确 `indonesia_network_map.v1` | 是 |
| 读取精确 `geojson.v1` | 是 |
| 调用 `map_utils.create_map_card` | 是 |
| 读取客户明细 | 否 |
| 重新计算网络方案 | 否 |
| 调用仓网优化 Tool | 否 |
| 创建下级 Agent | 否 |

Visualization Agent 的职责不是“做分析”，而是把已经验证的地图说明和 GeoJSON 交给
浏览器渲染能力。它的 `MAP_HANDOFF` 还会原样返回两个输入 Resource 名和地图 Artifact
ID，确保最终报告不会丢失地图来源。这样地图样式错误或来源断裂都不会静默改变网络结论。

如果你的领域已有满足职责和权限的 Agent Release，应直接复用；不要为了显示自定义名称
复制一份相同 Agent。

## 3. 发布三 Agent Supervisor 版本

在 `tutorial-indonesia-network-supervisor` 中点击 **New version**，创建 `3.0.0`。

Allowed Agents：

- `Tutorial Indonesia Data Agent · 1.0.0`；
- `Tutorial Indonesia Network Planner · 3.0.0`；
- `Enterprise Visualization Agent · 1.1.0`。

设置 **Maximum active child Agents = 3**。这是并发和驻留上限，不是要求每次都创建
3 个 Agent。

选择 handoff：

| Artifact | Producer | Consumer | Required |
| --- | --- | --- | --- |
| `indonesia_dataset_inspection.v1` | Data | Network | 否 |
| `indonesia_current_network_analysis.v1` | Network | Supervisor | 否 |
| `indonesia_candidate_scenario.v1` | Network | Supervisor | 否 |
| `indonesia_location_optimization.v1` | Network | Supervisor | 否 |
| `indonesia_network_map.v1` | Network | Visualization | 否 |
| `geojson.v1` | Network | Visualization | 否 |
| `map.v3` | Visualization | Supervisor | 否 |

**Custom Supervisor instructions**：

```text
你是 Root Supervisor，对最终印尼仓网决策报告负责。先整理用户目标和当前 Task 已有证据，
只调用能够补齐未解决证据缺口的已绑定 Agent。不要以固定角色数量、固定顺序、固定城市
或固定 Tool 次数作为完成条件。

Data Agent 只负责精确 Dataset inspection；Network Agent 负责当前网络、完整有限候选
优化和有界地图数据；Visualization Agent 只在已有兼容 map manifest 和 GeoJSON 且用户
要求地图时使用。Root 不调用业务 MCP、不读取 Workspace、不复制大数据，也不替代专业
Agent 计算。

所有跨 Agent 交接必须包含原样 schema、resource_name 和完整结构化 data_ref。缺失时向
原生产者请求补全；不得列出 Resources、猜 URI 或按名称搜索替代 Artifact。Tool 或 Agent
失败时保留已完成证据并报告未完成部分。

最终报告分为事实、假设、分析、建议、局限、缺失证据。每个关键数字引用准确 schema 和
resource_name，不暴露 URI。明确说明优化只覆盖 20 个候选，距离是球面距离乘系数，不是
导航承诺。

如果 Visualization Agent 返回 `MAP_HANDOFF` 和地图嵌入指令，证据索引必须复制其中的
map manifest、GeoJSON Resource 名和地图 Artifact ID；嵌入指令必须逐字保留，并作为
前后空行分隔的独立段落输出。不得放进代码围栏、行内代码、列表、引用、表格或 HTML。
```

完成：

```text
Save draft → Validate → Publish
```

Supervisor 没有写死“Data → Network → Visualization”，所有 handoff 都是条件性的。
本篇 Prompt 同时要求数据核验、网络优化和地图，所以一个全新 Task 通常会自然形成这条
依赖路径；如果问题不要求地图，Visualization Agent 应被跳过。同一 Task 的后续问题
可复用仍有效的 inspection；当前版本不宣称支持跨 Task Artifact 复用。

## 4. 启动完整案例

选择 **Tutorial Indonesia Network Supervisor · 3.0.0**，发送：

```text
分析现有印尼仓库网络并给出建议。

我需要知道当前网络的一日、两日和三日需求覆盖率、全网干线与末端年度运输成本、各省
时效表现，并按数据合同声明的省份排名口径分别列出表现最好和最需要改善的前三个省份。
然后判断能否在完整的已审核候选点中新增一个前置仓，使两日需求覆盖率达到 74%；建设
费用按 5 年摊销。请比较当前方案与入选方案的一日、两日、三日覆盖率，以及干线、末端、
运输总成本、年度固定成本、摊销建设成本和年度决策总成本，并创建一张地图展示两者差异。

请根据尚未解决的证据问题动态决定需要哪些 Agent，不要为了凑数量运行角色，也不要按
写死的 Agent 顺序执行。只使用平台授权的数据与类型化 Artifact；不要扫描 Workspace、
运行终端命令、读取原始客户行、调用导航服务或自行编造候选地点。距离采用教程声明的
球面距离乘系数方法，司机每天可行驶 6 小时。

最终报告先给出“证据索引”，再使用“事实、假设、分析、建议、局限、缺失证据”六个
部分。证据索引要说明每个原样 resource_name 负责哪些事实。每个关键数字注明拥有该
字段的 Artifact schema 和原样 resource_name，不要用服务基线引用成本，不要用入选
方案引用完整候选集合，也不要暴露 Resource URI。删除没有精确前后字段支撑的运营效果
推断。优化状态必须原样保留 Tool 枚举；金额保留精确 IDR 整数，不要换算为 B、million、
billion、万或亿，也不要自行计算新比例。如果已生成地图，必须原样嵌入地图交付指令。
```

## 5. 审阅动态执行

这个完整问题通常需要三个角色，但正确性不由精确调用顺序决定。检查：

- Root 没有 Shell、文件扫描或业务 MCP 调用；
- Data 只调用 inspection/validation 能力；
- Network 只使用同一 inspection Resource；
- 优化 Tool 的 `evaluated_candidate_count` 为 20；
- `target_met_candidate_count` 为 6，并且与 evaluations 中 `target_met=true` 的数量一致；
- 入选 ID 为 `CAN-JAMBI`，状态为 `target_met`；
- Visualization 只收到 map manifest 和 GeoJSON，不收到客户数据；
- 同一角色没有超过 spawn limit；
- Agent 的 running、waiting、completed 或 failed 状态有稳定身份和非空说明；
- 任一失败都形成明确终态，不永久停在空白 wait。

允许的变化：

- 独立信息核对可以并行；
- Agent 可以先读取 handoff 再调用 Tool；
- Root 可以向同一个已创建 Agent 追加补充问题；
- 同一 Task 已有有效 Artifact 时可以跳过对应生产者；
- 不要求模型额外调用独立 validator，因为 Domain producer 已原子校验。

## 6. 审阅 Artifact 与地图

完整任务至少应产生以下六个 Resource Artifact：

```text
indonesia_dataset_inspection.v1
indonesia_current_network_analysis.v1
indonesia_location_optimization.v1
indonesia_candidate_scenario.v1
indonesia_network_map.v1
geojson.v1
```

如果 Supervisor 选择了独立服务基线 Tool，还会出现
`indonesia_service_baseline.v1`。它是条件性证据，不是为了满足固定流程而强制执行。
最终报告还必须嵌入 Visualization Agent 交付的一个 `map.v3` 卡片；该卡片属于回复
交付，不要用 Agent Activity 中 Resource Artifact 的数量推断它是否存在。

地图 GeoJSON 是有界证据，只包含：

- 当前中心仓和前置仓；
- 入选 Jambi 候选仓；
- 当前和候选干线；
- 省级需求中心和时效差异摘要。

它不包含 240,000 个客户点，feature 数量必须小于 200。

`map_utils.create_map_card` 成功只创建 Artifact。地图真正显示还要求 Root 最终消息把
返回的嵌入指令作为独立段落输出。以下写法会被安全解析器拒绝：

```text
代码围栏中的指令
列表项中的指令
引用块中的指令
模型重新拼出来的近似指令
```

如果浏览器尚未配置公开 Mapbox Token，卡片仍应恢复为 `Ready`，并显示摘要、图例和
“配置 Mapbox Key”操作，但不会绘制地理底图。这是明确的凭据缺口，不是分析失败。

## 7. 审阅建议

正确建议应表达：

- 当前 2 天需求覆盖率为 71.80%，低于 74% 目标；
- Jambi 将其提升到 75.33%，目标成立；
- Jambi 让运输成本每年下降约 2.84 billion IDR；
- 加上固定费用和开仓摊销后，年度决策成本增加约 13.59 billion IDR；
- 因此它是“达到本目标的最低年度决策成本候选”，不等于“节省总成本”；
- 是否投资还需要服务改善的商业价值、实施可行性和真实路线复核。

Supervisor 可以建议进入更详细的商业论证，不能把教程计算当成最终投资批准。

## 8. 把方法迁移到真实场景

真实项目需要替换：

- 合成客户、仓库、分配、容量和报价；
- 规划系数和服务承诺口径；
- 候选点生成和审核流程；
- 开仓、固定和资本成本模型；
- 数据时效、质量阈值和审批规则。

可能需要新增独立 Agent：

- Finance Agent：只有当现金流、折现和投资回报会改变建议时；
- Risk Agent：只有当实施风险、数据不确定性或敏感性需要独立登记时；
- Routing Agent：只有当入围路线需要有缓存的导航矩阵时。

不要因为未来可能需要，就把这些职责全部写进当前 Supervisor。先定义清晰输入、
确定性 Tool、交付 Artifact 和停止条件，再发布新的 Agent Release。

完成后返回：

[教程标准与学习路径](README.md)

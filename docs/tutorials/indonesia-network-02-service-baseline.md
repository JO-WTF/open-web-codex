# 印尼仓网 2：加入两级仓网和运输成本

本篇在第一篇基础上增加报价和两级网络：中心仓到 cross-docking 仓的干线，以及 cross-docking 仓到需求城市的末端。仍然不做候选仓优化。

## 增加了什么

| 第一篇 | 本篇 |
| --- | --- |
| 只看距离和时效 | 另外把 `cost_matrix` facet 构建为 `ready` |
| 只用已有仓位置 | 区分 `center` 和 `cross_docking` |
| 只有末端路线 | 识别 linehaul 和 last-mile 报价 |
| 只输出覆盖率 | 输出总成本、干线成本、末端成本和分仓成本 |

数据文件新增 `route-quotes.csv`：550 条已有仓到需求城市末端报价，另有 30 条中心仓到 cross-docking 仓干线报价；报价单位是 IDR/车，一车装 1 个需求单位。`existing-warehouses.csv` 中的 `warehouse_type` 和 `upstream_center_id` 用于判断两级关系。

## Web 操作

1. 在同一个 Workspace 上传 `route-quotes.csv`。如果上一教程已上传其他文件，不要重复上传相同内容。
2. 在原 Thread 发送：

   ```text
   在刚才的印尼仓网输入上增加两级运输成本分析。使用已有报价，不要用每公里费用替代。
   分别计算 center 到 cross-docking 的干线成本、cross-docking 到需求城市的末端成本、全网总成本，并按仓库展示。
   时效仍使用 1.28 绕路系数、42 km/h 和每天 6 小时。没有报价的路线要明确列出，不要填零。
   ```

3. 如果输入卡片要求确认报价字段，按下表确认：

| 目标字段 | 报价字段 |
| --- | --- |
| `origin_id` | `ori_city_id` 或 `origin_id` |
| `destination_id` | `dest_city_id` 或 `destination_id` |
| `layer` | `linehaul` / `last_mile` |
| `price_per_vehicle` | `price` 或 `price_per_vehicle` |
| `currency` | `currency`，教程为 `IDR` |
| `vehicle_capacity` | 教程为 `1` |

## 为什么要分别计算两层

两层不是两个重复的末端成本：

```mermaid
flowchart LR
  C[中心仓] -->|linehaul 报价 × cross-docking 需求| X[cross-docking 仓]
  X -->|last-mile 报价 × 城市需求| D[需求城市]
```

如果一个城市由 cross-docking 仓服务，成本应按“干线一次 + 末端一次”累计；如果由中心仓直接服务，则只计对应末端路线。没有干线报价或上游关系时，工具必须返回缺口，不能把缺失当作 0。

## 预期 Agent 过程

- Data Agent 只增加报价和上游关系的映射，不重新扫描无关文件。
- Network Agent 调用 `plan_cost_matrix`，精确报价优先于 fallback rule。
- 有缺失报价时，Network Agent 应通过输入卡片询问固定费用、每公里费用或区域附加费；未得到规则前不继续成本汇总。
- `evaluate_network_baseline` 从 Case 的成本矩阵计算 linehaul、last-mile、总成本和分仓金额，不传递成本明细。
- 6、12、18 小时覆盖率由同一次显式目标分析计算；成本最优和时效最优仍是两组独立覆盖关系，不能混用。

## 真实检查

正确的成本结果应满足：

- Case 的 `cost_matrix` facet 为 `ready`，内部同时包含 `linehaul` 和 `last_mile` 行。
- 成本行保留 `source=quote`；没有报价而使用规则时才是 `source=calculated`。
- 1 个需求单位对应 1 辆车，成本为 `demand_quantity × price_per_vehicle`。
- 报告同时显示 `linehaul`、`last_mile`、`total`、`by_warehouse` 和完整性状态。
- 任何 `missing_routes` 都会让成本结果标为不完整，而不是伪造全网总成本。
- 时效报告仍不把优化基线称为实际当前方案。

## 自己接入报价

如果用户提供的报价不是逐路线表，可以只在输入卡片中提供明确规则，例如：

```text
固定起步价 45,000 IDR + 调整后公里数 × 1,450 IDR/km；车辆容量 1；币种 IDR。
```

规则必须说明单位和适用层级。不要让 Agent 从列名或历史报价猜出计费方式。

下一篇将加入 current coverage，解释“真实当前方案”和“已有仓范围内优化基线”的区别，并运行增删搬迁仓模拟。

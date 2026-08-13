# 印尼仓网案例总览

本案例把一个真实的仓网问题拆成四篇教程。目标不是让模型把所有原始数据读进上下文，而是让用户在 Web 中看到：哪些数据已经准备好、哪个 Agent 正在工作、哪里需要用户决定，以及每个结果由同一 `supply_chain` provider 中哪个 typed Resource 和确定性组件产生。

## 业务背景

我们要为印度尼西亚的需求城市规划仓库网络。教程 fixture 使用人口最大的 50 个城市作为需求点，需求量按 `ceil(population / 1000)` 生成；每个需求点都通过 geoBoundaries ADM2 边界检查。已有网络包含 5 个 center 仓和 6 个 cross-docking 仓：Jakarta、Palembang、Medan、Surabaya、Makassar 是中心仓，另外六个城市作为前置仓。候选仓、当前覆盖和报价分别作为后续步骤的可选输入。

教程默认不调用导航接口。距离使用 `haversine distance × detour coefficient`，时效使用调整后距离除以平均行驶速度，并按每天 6 小时驾驶换算日级时效。这个口径用于规划筛选，不是车辆级 ETA 承诺。真实客户量很大时，应先聚合到城市或代表点；只有路线规模、缓存、费用和许可都明确后，才接入批量导航矩阵。

## 数据与来源

fixture 位于 [indonesia-network/base](../../copilots/warehouse-network/tools/planner/examples/indonesia-network/base/)，包括：

- `demand-cities.csv`：50 个需求城市、人口、需求量、省份和坐标。
- `existing-warehouses.csv`：11 个已有仓及 center/cross-docking 类型和上游中心仓关系。
- `administrative-areas.json`：行政区、坐标和边界匹配结果。
- `route-quotes.csv`：550 条仓到需求城市的末端报价和 30 条中心到前置仓的干线报价。
- `candidate-warehouses.csv`：可用于后续选址的候选位置。
- `source-lock.json`、`validation-report.json`、`dataset-manifest.json`：来源、许可、hash 和生成质量门禁。

这些数据是可重放的教程数据，不是商业事实。生成器和校验报告共同证明城市点在边界内、需求公式正确、仓库数量正确、报价矩阵完整且报价与距离保持正相关。

## Agent 分工

Root Thread 先发布 typed 数据需求。Network Agent 持有业务参数，Data Agent 只负责发现和检查 CSV、JSON、XLSX，提出显式字段映射，补充行政区和坐标，并发布 `normalized_network_input.v1`；Network Agent 再从严格 `ResourceRef` 构建路线和成本矩阵，执行覆盖、成本、场景和选址分析。

Supervisor 不按固定阶段脚本运行。它根据当前 Resource 缺口和依赖决定是否需要另一个 Agent；无依赖任务可以并行，但没有必要输入时不能提前计算。原始表格只由工具在受限范围内读取，Agent 之间只传经校验的 typed `ResourceRef` 和有限摘要。最终报告才由 final Tool 创建为 Workspace Markdown 交付物。

## 四篇教程

1. [数据和球面时效](indonesia-network-01-data.md)：上传需求城市和已有仓库，得到路线矩阵、时效最优覆盖和 6/12/18 小时覆盖率。
2. [两级仓网成本](indonesia-network-02-service-baseline.md)：加入 center/cross-docking 关系和报价，分别计算干线、末端和全网成本。
3. [当前覆盖和场景](indonesia-network-03-two-level-cost.md)：加入 `current-coverage.csv`，区分 `actual_current` 与 `optimized_existing_footprint`，再模拟增删和搬迁。
4. [候选仓、选址和地图](indonesia-network-04-optimization-map.md)：加入候选仓，运行 p-median、时效约束选址，并生成确定性方案比较地图。

每篇都可以在 Web 中独立复查。教程只在用户明确写出“使用教程 mock 数据”时把 fixture 作为普通 Workspace 文件写入；本包没有独立 Demo MCP，空 Workspace、缺文件或真实工具失败都不会自动回退到示例数据。

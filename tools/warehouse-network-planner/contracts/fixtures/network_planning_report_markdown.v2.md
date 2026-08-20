# 仓网规划结果简报

<!-- network_planning_report_markdown.v2 -->

## 执行摘要

- 国家代码：`ID`
- 分析范围：50 个需求城市、11 个现有仓、12 个候选仓，需求总量 50,815。
- 变更后结果：最优方案（已证明最优），共 13 个启用仓。
- 仓网变动：新增 2 个仓，移除 0 个仓，重新分配 50 个需求城市。
- 成本：变更前 IDR 51,623,223,208.23，变更后 IDR 17,977,937,966.86，变化 IDR -33,645,285,241.37。

## 仓库变动

- 新增仓库：`WH-CANDIDATE-BALIKPAPAN`, `WH-CANDIDATE-BANDAR_LAMPUNG`
- 移除仓库：无
- 启用仓库数：变更前 11 个，变更后 13 个。

## 时效覆盖

| 时效目标 | 变更前城市覆盖率 | 变更后城市覆盖率 | 变更前需求量加权覆盖率 | 变更后需求量加权覆盖率 | 变化 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 6 小时 | 34.0% | 50.0% | 55.8% | 73.0% | +17.2% |
| 12 小时 | 44.0% | 68.0% | 67.3% | 82.3% | +15.1% |
| 18 小时 | 54.0% | 88.0% | 72.4% | 95.7% | +23.4% |

## 受影响的需求城市

共有 50 个城市受影响，其中 50 个城市重新分配。

| 需求城市 | 变更前仓库 | 变更后仓库 | 时长变化 | 单位成本变化 |
| --- | --- | --- | ---: | ---: |
| Jakarta (\`IDN-CITY-001\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-JAKARTA` | -0.50h | -103,563.90 |
| Surabaya (\`IDN-CITY-002\`) | `WH-CROSS_DOCKING-SEMARANG` | `WH-CENTER-SURABAYA` | -6.78h | -894,584.96 |
| Bekasi (\`IDN-CITY-003\`) | `WH-CROSS_DOCKING-BEKASI` | `WH-CENTER-JAKARTA` | +0.68h | -40,000.00 |
| Bandung (\`IDN-CITY-004\`) | `WH-CROSS_DOCKING-BANDUNG` | `WH-CENTER-JAKARTA` | +3.30h | -40,000.00 |
| Medan (\`IDN-CITY-005\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-MEDAN` | -40.04h | -2,594,050.51 |
| Depok (\`IDN-CITY-006\`) | `WH-CROSS_DOCKING-BEKASI` | `WH-CENTER-JAKARTA` | -0.17h | -93,418.14 |
| Tangerang (\`IDN-CITY-007\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-JAKARTA` | +0.50h | -40,000.00 |
| Palembang (\`IDN-CITY-008\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-PALEMBANG` | -11.71h | -809,463.82 |
| Semarang (\`IDN-CITY-009\`) | `WH-CROSS_DOCKING-SEMARANG` | `WH-CENTER-SURABAYA` | +6.78h | -40,000.00 |
| Makassar (\`IDN-CITY-010\`) | `WH-CROSS_DOCKING-SEMARANG` | `WH-CENTER-MAKASSAR` | -29.27h | -2,311,146.23 |
| Tangerang Selatan (\`IDN-CITY-011\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-JAKARTA` | -0.12h | -79,033.56 |
| Batam (\`IDN-CITY-012\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-PALEMBANG` | -11.35h | -786,531.55 |
| Bandar Lampung (\`IDN-CITY-013\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-JAKARTA` | +0.49h | -41,152.07 |
| Bogor (\`IDN-CITY-014\`) | `WH-CROSS_DOCKING-DEPOK` | `WH-CENTER-JAKARTA` | +0.30h | -57,305.72 |
| Pekanbaru (\`IDN-CITY-015\`) | `WH-CROSS_DOCKING-TANGERANG` | `WH-CENTER-MEDAN` | -13.29h | -909,109.39 |
| Padang (\`IDN-CITY-016\`) | `WH-CROSS_DOCKING-SOUTH_TANGERANG` | `WH-CENTER-PALEMBANG` | -10.89h | -746,163.67 |
| Malang (\`IDN-CITY-017\`) | `WH-CROSS_DOCKING-SEMARANG` | `WH-CENTER-SURABAYA` | -5.06h | -786,374.19 |
| Samarinda (\`IDN-CITY-018\`) | `WH-CROSS_DOCKING-SEMARANG` | `WH-CENTER-MAKASSAR` | -12.87h | -1,278,044.98 |
| Denpasar (\`IDN-CITY-019\`) | `WH-CROSS_DOCKING-SEMARANG` | `WH-CENTER-SURABAYA` | -6.44h | -872,844.45 |
| Tasikmalaya (\`IDN-CITY-020\`) | `WH-CROSS_DOCKING-BANDUNG` | `WH-CENTER-JAKARTA` | +3.29h | -40,158.25 |

_另有 30 个变更城市保留在结构化计算结果中。_

## 成本汇总

| 情景 | 总成本 | 干线成本 | 末端成本 | 数据是否完整 |
| --- | ---: | ---: | ---: | :---: |
| 变更前 | IDR 51,623,223,208.23 | IDR 9,611,905,065.56 | IDR 42,011,318,142.67 | 是 |
| 变更后 | IDR 17,977,937,966.86 | IDR 0.00 | IDR 17,977,937,966.86 | 是 |

## 说明

- 完整的仓库—需求城市对应关系属于结构化计算结果，不在本简报中重复列出。
- 本次求解没有额外提示。

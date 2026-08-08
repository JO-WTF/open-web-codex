# Supply Chain Network Planner

这个 Codex Plugin 提供通用仓网规划能力。Codex Runtime 负责 Thread、Turn、Agent、Skill、MCP 和模型执行；本包只负责经过审查的 Data/Network 工具和持久化 Resource。

## MCP 入口

本包只注册三个入口，均由同一个 launcher 启动，并使用隔离 Python 环境：

| Server | 责任 |
| --- | --- |
| `supply_chain_data` | 发现和检查授权 Workspace 的 CSV/JSON/XLSX，发布来源画像、映射、标准化输入和行政区结果 |
| `supply_chain_planner` | 构建路线/成本矩阵，计算覆盖、成本、场景、p-median、服务约束选址、比较地图和报告 |
| `supply_chain_demo` | 只有用户明确要求 mock/demo/tutorial 时，向空 Workspace 复制已验证的印尼教程 fixture |

没有独立的 Indonesia MCP。国家由用户问题确定，行政区能力属于 `supply_chain_data`；地图由 Network Agent 使用 Planner 的确定性地图工具生成。

## 当前网络工具

Network Agent 的主要工具是：

```text
plan_route_matrix
build_haversine_route_matrix
register_navigation_route_matrix
validate_route_matrix
plan_cost_matrix
compute_optimal_assignment
evaluate_network_baseline
evaluate_service_targets
summarize_network_cost
evaluate_facility_scenario
solve_p_median
solve_service_constrained_location
compare_network_scenarios
render_network_comparison_map
publish_network_planning_report
```

每个大数据结果都写入 Resource Store。Agent 消息只传 `ArtifactRef` 或工具返回的精确引用，不传原始文件、整张矩阵、完整工具结果或内部路径。缺失数据、矩阵不完整、求解器不可用和超时都是显式状态；不会自动加载 Mock、填零或切换旧实现。

## 6.0 能力包

| Package | 责任 |
| --- | --- |
| `enterprise-data-agent@6.0.0` | 来源发现、字段映射、行政区解析、坐标和边界校验、按需标准化 |
| `enterprise-network-planning-agent@6.0.0` | 需求定义、路线/成本口径、覆盖、成本、场景、选址和地图/报告 |
| `enterprise-supervisor-copilot@6.0.0` | 动态协调 Data 和 Network，不写死国家、阶段数量或调用顺序 |

Supervisor 不绑定 Visualization Agent。用户输入统一通过 Runtime 官方 `requestUserInput`，平台将 root 和 child 请求投影到同一个 Run 输入队列。

## 印尼教程 Fixture

已验证数据位于：

```text
examples/indonesia-network/base/
examples/indonesia-network/current-coverage-extension/
examples/indonesia-network/candidate-extension/
```

基础 fixture 包含 50 个需求城市、5 个中心仓、6 个 cross-docking 仓、550 条末端报价和 30 条干线报价。需求量是 `ceil(population / 1000)`；城市点位经过 geoBoundaries ADM2 point-in-polygon 校验；报价与距离的 Spearman 相关系数不低于 0.85。`current-coverage-extension` 只在用户需要实际当前方案时使用。

教程生成器会保存来源 URL、获取日期、许可证、原始 hash 和生成器版本：

```bash
python3 tools/supply-chain-network-planner/scripts/generate_indonesia_tutorial_data.py \
  --network-fixture
```

生成器不访问模型、不修改 Runtime，也不会把 fixture 作为空 Workspace 的自动回退。

## 开发和验证

```bash
./tools/supply-chain-network-planner/bin/setup-env
PYTHONPATH=tools/supply-chain-network-planner \
  .local/open-web-codex/tool-envs/supply-chain-network-planner/bin/python \
  -m pytest tools/supply-chain-network-planner/tests -q
PYTHONPATH=tools/supply-chain-network-planner \
  .local/open-web-codex/tool-envs/supply-chain-network-planner/bin/python \
  -m ruff check tools/supply-chain-network-planner
```

真实 stdio smoke 只在明确设置 `RUN_REAL_STDIO_SMOKE=1` 时启动三个 MCP 入口。任何 MCP Tool 都通过 Runtime 的 MCP 调用路径执行，禁止通过 shell `source`、目录遍历或手工拼接 Resource URI 调用。

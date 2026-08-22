# 开发一个仓网 Copilot

这篇给开发者看：怎样在本平台中创建或扩展一个可运行的仓网 Copilot。

先分清两件事：

- **使用 Copilot**：在 Web 上传数据、创建任务、提问题；
- **开发 Copilot**：在仓库中编写领域工具、业务指引和角色配置，验证后由平台在本地启动时发现。

当前阶段没有 Web 页面让你在线编辑、发布 Copilot。开发路径是：**源码 → SDK 验证 → 本地平台启动 → Web 中选择并运行**。

## 你最终会做出什么

一个完整 Copilot 由四部分组成：

```text
业务问题
  ├── Skill：告诉助手何时该做什么、何时该问用户
  ├── Agent Role：决定每个角色能使用哪些 Skill 和工具
  ├── Domain Tool：读取数据、计算时效/成本/选址、生成地图
  └── copilot.toml：把以上内容和交付结果组合成一个可选的 Copilot
```

平台负责通用的 Workspace 授权、任务、对话、工具运行、浏览器展示和最终交付。仓网 Copilot 只负责仓网业务，不应把业务字段、计算状态或调度逻辑写进 Platform。

## 0. 先决定：扩展现有 Copilot，还是新建一个？

| 你的目标 | 正确做法 |
| --- | --- |
| 给现有仓网增加一个数据规则、计算或地图样式 | 扩展 `warehouse-network-planner` 或 `warehouse-network-maps`，并补测试 |
| 仍是仓网问题，但希望一个助手独立完成 | 使用或扩展 `warehouse-network-single-agent` |
| 需要把数据准备和规划分开，且会连续追问 | 使用或扩展 `warehouse-network` |
| 是一个不同领域，例如配送承诺、库存补货 | 新建独立 Copilot 和独立领域 Tool 包 |

不要为了“多一种入口”给同一个 Copilot 增加 mode。一个 Copilot 只有一个 Root。单 Agent 和多 Agent 是两个独立包，用户在创建任务时明确选择其中一个。

## 1. 认识仓网参考实现

从这两个目录开始读：

```text
copilots/warehouse-network/                # 多 Agent：Root + Data + Network
copilots/warehouse-network-single-agent/   # 单 Agent：一个 Root 完成全部工作

tools/warehouse-network-planner/           # 数据准备、路线、成本和选址计算
tools/warehouse-network-maps/              # 地图卡片和样式
```

前两个目录只放 Copilot 的组合和业务说明：`copilot.toml`、`skills/`、`agents/` 和测试。后两个目录才是可复用的领域计算。不要复制一份 Planner 到每个 Copilot；同一计算规则应只有一个 owner。

通用 SDK 位于 `packages/copilot-sdk` 和 `packages/copilot-provider-sdk`。它们是平台基础设施，不放业务规则，也不依赖仓网。

## 2. 从一个明确业务问题开始

先写一句使用者会说的话。例如：

```text
根据上传的需求城市、现有仓和路线报价，计算 24 小时时效达标率，
并比较新增一个指定候选仓后的变化。
```

然后把它拆成四个稳定事实：

| 问题 | 负责者 |
| --- | --- |
| 什么文件是需求、仓库、候选仓和路线 | 数据 Tool |
| 怎样计算时效、成本和新增仓变化 | 规划 Tool |
| 什么时候需要业务人员选择 | Skill |
| 怎样把结论和地图交给用户 | Copilot 的 delivery 声明 |

模型不负责手算、猜字段或补造数据。它只理解目标、选择已经声明的能力、收集无法推断的业务决定，并用业务语言解释结果。

## 3. 创建 Copilot 源码骨架

安装本仓库的两个通用 SDK：

```bash
python3 -m pip install -e packages/copilot-provider-sdk -e packages/copilot-sdk
```

创建一个最小骨架：

```bash
copilot init ./scratch/my-warehouse-copilot --name my-warehouse-copilot
```

这个命令会创建可以验证的起点，不会替你写出仓网算法。新项目可以先使用自己目录内的 Tool；如果某个领域 Tool 要被多个 Copilot 复用，应把它做成根目录 `tools/<tool-package>/` 下的共享包，再由每个 Copilot 显式引用。

## 4. 编写领域 Tool：让计算有唯一主人

领域 Tool 放在 `tools/<tool-package>/`，至少包含：

```text
tool.toml        # 包 ID 和运行声明的位置
runtime.toml     # 依赖、MCP server、超时和允许注入的环境变量
pyproject.toml   # Python 包定义；或 package.json
requirements.lock / package-lock.json
src/             # Tool 的实现和测试
```

Tool 要做确定性的工作：读取已授权 Workspace 数据、验证输入、计算、产生结构化结果。它应返回明确的三种状态：成功、需要用户补充、无法继续。

例如仓网 Tool 应负责：

- 将 CSV、Excel 和 JSON 中被选中的表整理成标准输入；
- 检查路线、报价和必要单位；
- 计算时效、成本、场景变化和选址；
- 从已确认的分配结果生成地图数据。

不要把这些规则写在 Skill 里，也不要让 Platform 数据库保存第二份“仓网状态”。Tool 也不能擅自覆盖用户原始文件；需要生成内容时，写入约定的输出目录并使用新文件名。

如果 Tool 需要平台提供的通用 Python 能力，在 `pyproject.toml` 和 `runtime.toml` 中都显式声明 `open-web-codex-provider-sdk`。不要使用源码路径、`PYTHONPATH`、私有安装脚本或另一个 MCP transport。

## 5. 编写 Skill：写业务判断，不写系统流程

Skill 位于 Copilot 的 `skills/<skill-id>/SKILL.md`。好的仓网 Skill 只回答四件事：

1. 这次业务目标需要哪些数据；
2. 字段清楚时怎样继续，什么情况必须询问；
3. 哪个已准备好的结果可以继续使用；
4. 最终回答要怎样解释数字、限制和地图。

例如：缺少 `warehouse_type` 且无法从其他字段确定时，应要求用户选择；只是列名改成中文或缩写时，应让数据 Tool 映射，而不是一律阻塞。

不要在 Skill 中写：工具名称的固定调用顺序、文件路径规则、权限判断、重试循环、缓存协议或平台错误码。这些分别由 Tool、Role 和 Runtime 管理。Skill 也不应要求每个新对话都重新发现已经可用的工具；只有当前确实没有需要的工具时，才使用原生 Tool Search。

## 6. 编写 Agent Role：最小权限、清楚分工

Role TOML 位于 `agents/`。它决定一个角色可用的 Skill、MCP server 与审批方式。

多 Agent 仓网参考：

| 角色 | 能做什么 | 不做什么 |
| --- | --- | --- |
| Root | 理解目标、协调、询问业务选择、汇总结论 | 不读原始文件，不直接计算 |
| Data | 发现和准备用户数据 | 不算时效、成本或地图 |
| Network | 路线、分析、选址、地图交付 | 不重读原始表格 |

单 Agent 版本只有一个 Root；它同时拥有数据、规划和地图能力，但仍要按相同的业务边界工作。

`developer_instructions` 应保持很短，只写角色边界和安全底线。完整的业务方法放在 Skill。不要让 Root 获得所有领域工具来“以防万一”，也不要用提示词扩大权限。

## 7. 用 `copilot.toml` 组合起来

`copilot.toml` 是一个 Copilot 的入口。下面是引用根级共享 Tool 的最小形状：

```toml
schema_version = 1
id = "my-warehouse-copilot"
display_name = "My Warehouse Copilot"

[root]
skill = "warehouse-root"
agent = "warehouse_root"
task_skills = "all"

[[skills]]
id = "warehouse-root"
path = "skills/warehouse-root"

[[agents]]
id = "warehouse_root"
role = "agents/warehouse_root.toml"

[[tools]]
id = "supply_chain"
package = "warehouse-network-planner"
```

若要交付给浏览器展示的报告或地图，在同一文件里添加 `[[deliveries]]`，并精确写出由哪个 server 和 Tool 产生。Platform 只根据这个声明登记最终交付；它不会从模型文字或普通中间结果猜测“这是一张地图”。

使用共享 Tool 时，验证命令必须带上根级 Tool registry：

```bash
copilot validate copilots/my-warehouse-copilot --tool-registry-root tools
```

## 8. 逐层验证，不要跳过

按这个顺序验证，每一步只回答一个问题：

| 命令 | 它证明什么 |
| --- | --- |
| `copilot validate` | 组合、路径、Skill、Role 和 Tool 引用是否一致 |
| `copilot prepare` | Tool 依赖能否在平台外置环境中准备好 |
| `copilot dev` | 真实 Codex Runtime 能否发现声明的 Skill 和 MCP server |
| `copilot test` | 本地确定性 Provider 下，一条原生 Role/Tool 正常链是否完整 |

建议命令：

```bash
copilot validate copilots/my-warehouse-copilot --tool-registry-root tools
copilot prepare copilots/my-warehouse-copilot --tool-registry-root tools \
  --output-root /absolute/path/to/prepared-copilots
copilot dev copilots/my-warehouse-copilot --workspace "$PWD" \
  --tool-registry-root tools
copilot test copilots/my-warehouse-copilot --workspace "$PWD" \
  --tool-registry-root tools
```

`dev` 不会调用模型，它验证发现链；`test` 使用本地确定性 Provider 验证一条真实运行链。两者都不是生产模型质量证明。

## 9. 交给本地平台发现

在这个仓库中，不需要手工写数据库记录、修改浏览器配置或把 Tool 复制到 Profile。把
Copilot 放在受信任的 `copilots/<package-id>/` 目录，并由本地启动脚本准备环境：

```bash
scripts/run-local.sh
```

启动脚本会先验证并准备所有配置好的 Copilot；准备成功的包才会出现在 Web 的新任务选择列表。
如果某个包的声明或依赖有问题，它会明确显示为不可用，而不是让浏览器悄悄选择另一个包。

## 10. 在 Web 做一次真正验收

本地服务启动并发现 Copilot 后：

1. 在 Web 创建新的 Workspace；
2. 上传一组真实或明确标注的测试数据；
3. 新建任务，确认能在列表中选择新 Copilot；
4. 提一个真实业务问题；
5. 验收数据缺口是否用输入卡片询问、错误是否明确停止、最终报告或地图是否展示；
6. 在同一个任务里追问一次，确认同一角色和已准备数据能继续使用。

仓网参考验收可以直接复用三问：12 小时时效、新增 Balikpapan、90% 最少新增仓。重点验收业务结果和地图，而不是要求模型按某个固定内部顺序调用工具。

## 发布前检查表

- [ ] 一个 Copilot 只有一个 Root；单/多 Agent 是不同的 package；
- [ ] 业务计算只有一个 Tool owner，没有复制到 Skill、Role 或 Platform；
- [ ] Role 只启用完成任务所需的 Skill 和 MCP server；
- [ ] 所有输入、成功、需要补充、失败和取消都有明确结果；
- [ ] 用户数据不会被覆盖或静默补造；
- [ ] 最终报告和地图有精确 delivery 声明；
- [ ] `validate`、`prepare`、`dev`、`test` 和 Web 真实任务都通过；
- [ ] 文档面向业务用户时不泄露内部路径、协议、凭据或工具名称。

命令参数的逐项说明见[Copilot 开发者快速开始](copilot-developer-quickstart.md)。完整仓网参考工程见[多 Agent 包](../../copilots/warehouse-network/)和[单 Agent 包](../../copilots/warehouse-network-single-agent/)。

# open-web-codex 开发计划

## 当前状态

| 字段 | 内容 |
| --- | --- |
| 更新日期 | 2026-08-08 |
| 当前阶段 | Gate 0 + M2 单 Profile Copilot 创作平台 |
| 当前目标 | 用户通过 Web/SDK 创建并运行 Tool + 中文 Skill + Domain Agent + Supervisor Copilot |
| 目标架构 | `docs/supervisor-agent-skill-tool-architecture.md` |
| 详细实施 | `docs/agent-capability-lifecycle-plan.md` |
| 当前事实 | `docs/architecture.md`、`docs/capability-baseline.md` |
| 临时审计 | `docs/temporary-copilot-platform-refactor-audit-2026-08-08.md` |
| Codex 策略 | 默认不修改；只复用官方合同、包机制和已登记 retained seam |

本文只维护当前工作、阻塞、下一批任务和验收。长期阶段顺序由 `docs/roadmap.md` 负责；
类和方法级任务由详细实施计划负责；已验证能力只进入能力基线。

## 1. 当前目标解释

当前不再以“把仓网案例跑通一次”作为终点。当前阶段要建立一个单用户、单 Profile 的
平台化 Copilot 创作与运行闭环：

1. 算法工程师通过受限 SDK 创建规范化 Python MCP Tool package；
2. Tool 经测试、发布、安装和 Runtime discovery 后可被 Agent 使用；
3. 用户在 Web 用中文创建 Skill，声明方法、输入、工具、失败处理和交付件；
4. 用户组合 Skills、Tools、数据权限和交付合同创建 Domain Agent；
5. 用户组合精确 Agent Releases 创建 Supervisor/Copilot；
6. Root 通过平台只读协调能力观察 Agent、输入、共享工作状态和交付件；
7. 印尼仓网和第二个非供应链案例共同验证扩展性。

多用户仍是后续开放门禁，但所有新增资源、缓存、事件和进程键必须从现在开始携带
organization/user/profile/workspace/task/run scope。

## 2. 当前实现事实

当前工作树已经完成一轮仓网 6.0 纵向重构，但尚未成为目标平台架构。

### 已实现且有局部证据

- 仓网不再把 `planning-dataset.v2` 作为所有分析的统一前置；
- Data/Network Agent 通过一个 `case_id` 和有界摘要协作；
- Tool result 限制为 16 KiB，大型业务数据留在 Case store；
- 当前包只注册一个供应链 MCP 入口和精确 Tool allowlist；
- 子 Agent 官方 `request_user_input` 可持久化到 root Run，并在 Web 同时显示多个卡片；
- Agent execution 有稳定标题、等待轮数、等待输入、单一终态和安全摘要；
- Supervisor Draft 使用 revision，发布时服务器分配版本；
- 50 城市、11 仓和报价 fixture、矩阵、成本、baseline、场景、求解、报告和地图已有
  Python 实现；
- Mock 只允许显式 demo 意图，不能作为失败回退；
- DeepSeek cache usage 字段已被 Adapter 解析，但真实缓存命中尚未证明。

### 当前验证证据

- Supply Chain Python：95 项测试通过，Ruff 通过；
- Web：188 个测试文件、1333 项测试、typecheck、lint、build 通过；
- Supervisor Catalog：16 项测试通过；
- Approval Service：5 项测试通过；
- Server execution projection：26 项通过，1 项真实 PostgreSQL 测试未执行；
- Codex API：165 项测试通过；
- Tutorial contracts 和 package hash 聚焦回归通过。

这些是局部证据，不代表当前 6.0 真实 Runtime/Web E2E 已完成。

## 3. 当前阻塞与根因

### P0

1. **服务当前停止。** 已应用 migration `20260807000045` 在工作树中被改写，SQLx 拒绝
   启动。Provider-only 备份因本机 `pg_dump 17` 与 PostgreSQL 18 不匹配而在破坏操作
   之前停止；数据库仍完整。
2. **真实扩展 E2E 未运行。** 当前 6.0 没有证明 Data/Network 子 Thread、输入、两类
   baseline、场景、选址、地图、刷新和重启恢复能在真实 `/web` 收敛。
3. **Data Intake 双重 owner。** Platform 已拥有 SourceAsset、DataIntakeSession、mapping
   和 Dataset Release，供应链 Case 又保存来源和映射状态。
4. **领域协议泄漏。** Platform event projection 显式认识
   `network-case-tool-result.v1`。

### P1

1. Case 的 revision、operation、dependency、stale、readiness 和 deliverable 是通用机制，
   却由供应链包实现；
2. Root 缺少平台权威、Task scoped、只读 coordination Tool，仍需依赖子 Agent 自述；
3. Assignment 的目标、共享状态、能力和交付主要由自然语言约定；
4. Agent/Supervisor/Tutorial 的版本、hash、Runtime Role 和依赖仍有手工维护路径；
5. Tool、Skill、Agent、Supervisor 没有统一 Catalog/Release/Installation 生命周期；
6. Provider 调用缺输入、缓存、输出、Tool schema、延迟和 compaction 的逐调用观测。

### P2

1. 供应链 `server.py` 保留未注册的旧 Resource/snapshot/solver/report 代码；
2. 导航矩阵只有规划和费用确认，缺当前 ToolPackage 下的结果注册闭环；
3. 部分 current-state 文档仍有旧版本和旧合同；
4. 只有仓网案例，尚不能证明抽象适用于第二领域；
5. 多用户隔离矩阵尚未执行。

完整映射和解决方案见详细实施计划第 1 节。

## 4. 当前执行顺序

### 当前批次 A：Gate 0 基线恢复

状态：下一步执行，阻塞真实集成验证。

1. 找到或安装 PostgreSQL 18 client；
2. 只导出 Provider Definition 和加密 Secret 关联，校验无明文；
3. 显式重建开发数据库并从当前 migrations 初始化；
4. 恢复 Provider 配置，连续启动两次；
5. 增加 migration checksum CI 和显式开发库重建脚本；
6. 运行 Rust/Web/Python/真实 Runtime 基线测试。

不得在 startup 添加 checksum 忽略、自动改表、旧 migration 双读或 Provider 重建默认值。

### 当前批次 B：公共平台合同

状态：A 后立即开始；文档设计已完成。

1. 在 `platform-contracts` 新增 Catalog、Work State、Collaboration、Tool Result、
   Installation 和 Observability DTO；
2. 建立 `work-state-service` 与数据库 schema；
3. 建立 Python Tool SDK 的 `platform-tool-result.v1`；
4. 建立 Task/Run scoped Root Coordination Tool；
5. 删除 event projection 的仓网 envelope 分支。

批次退出：一个无供应链字段的 fixture 能创建 Work State、提交 operation/component、由
Root 查询 execution/blocker/deliverable。

### 当前批次 C：Data Intake 收敛

状态：依赖 B。

1. 把 `routes/data_intake.rs` 状态机下沉到 `data-intake-service`；
2. 发布类型化 `DataRequirementContract`；
3. 建立领域 validator/normalizer adapter；
4. Dataset Release 绑定 Work State component；
5. 删除供应链 case_sources、mapping candidate/selection 和 Workspace 扫描 Tool。

批次退出：供应链只接收授权 Dataset Release handle，不再保存第二份文件/映射生命周期。

### 当前批次 D：协作、版本和安装

状态：依赖 B、C。

1. 实现 CollaborationContextBuilder 和 AssignmentCompiler；
2. 将 execution reducer 收敛为完整生命周期；
3. 重构 `supervisor-catalog` 为统一 `capability-catalog`；
4. 实现 CopilotPackageCompiler；
5. 实现 Profile Installation 与 Runtime discovery readiness；
6. 删除手工 semver/hash/role/MCP lock 路径。

批次退出：代码 seed、Web Draft 和 Tutorial 使用同一 compiler；运行固定 installation
snapshot；published/install/discovered/ready 可以独立失败。

### 后续批次 E：Studio 和参考实现

1. Tool SDK/Studio；
2. 中文 Skill Studio；
3. Agent Studio、Supervisor Studio 和 Copilot Builder；
4. Provider context/cache observability；
5. 仓网迁移并完成真实扩展 E2E；
6. 第二个非供应链案例；
7. 两用户隔离门禁。

具体类、方法、表、删除项和测试按实施计划 W6-W13 执行。

## 5. Codex 修改门禁

当前目标优先通过以下顺序实现：

1. 官方 app-server V2 和 Runtime discovery；
2. 官方 Skill、Plugin、MCP 配置/包机制；
3. Platform Catalog、Compiler、Work State、Data Intake 和 Profile Host；
4. 当前 Patch Map 已登记的 exact role seam；
5. 只有“安全安装用户 Agent 并让受治理 Thread 精确发现”无法由以上能力实现时，才提出
   新 Codex seam。

在任何新 `codex-rs` 修改前必须：

```bash
scripts/codex-upstream-status.sh
scripts/codex-customization-status.sh
```

并更新 `docs/custom-codex-patch-map.md`，写明官方能力不足证据、最小 owning crate、类型
合同、验证、重放顺序和退出条件。Web 卡片、Catalog、Work State、Data Intake、Artifact
和版本发布不能成为修改 Codex 的理由。

## 6. 当前不做

- 不建设第二个 Agent Scheduler、Thread Store、Memory 或 context compactor；
- 不在当前阶段开放成员、邀请、租户管理和跨组织 Catalog UI；
- 不把 Network Case 本身提升为通用平台对象；
- 不允许任意 shell/任意依赖构建成为 Tool 安装路径；
- 不用长 Prompt 代替权限、能力、输入和交付合同；
- 不通过名称、路径、版本字符串、文件存在或错误文本推断 capability；
- 不保留 2.x/5.x/6.0 项目合同兼容；
- 不用 retry、延长 timeout、Mock、旧版本 fallback 或扩大上下文掩盖失败。

## 7. 当前完成门

本阶段不能仅以“仓网返回一份报告”为完成。必须同时满足：

1. 用户通过 Web/SDK 创建 Tool、中文 Skill、Agent、Supervisor 和 Copilot；
2. Root 只读观察可信状态，Runtime 仍拥有调度；
3. Data Intake、Work State、Catalog、Installation、Runtime discovery 各有唯一 owner；
4. 大型数据通过引用交换，Provider 指标能解释上下文与缓存；
5. 仓网和第二案例都不需要 Platform 领域分支；
6. 正常、失败、取消、超时、刷新、重启、乱序和并发有证据；
7. 当前能力基线更新；
8. Codex 没有新增未登记差异。

临时审计文档只在上述平台迁移和仓网真实 E2E 完成前保留，之后删除。

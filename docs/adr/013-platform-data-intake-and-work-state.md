# ADR-013：Platform Data Intake 与通用 Work State 分工

状态：已被 [ADR-018](018-built-in-network-copilot-runtime-closure.md) 替代（2026-08-08）；
下文仅保存历史决定，不得作为当前实现合同。

替代：[ADR-010](010-workspace-wide-intake-discovery.md)，并替代
[ADR-009](009-thread-first-data-intake.md) 中由领域 Agent 扫描 Workspace、保存映射状态和
依赖单体 planning Dataset 的部分。

## 背景

仓网 Case 同时保存来源、映射、component revision、operation、dependency、readiness
和 deliverable。Platform 已经拥有 SourceAsset、DataIntakeSession、mapping 和 Dataset
Release。这造成同一文件和映射有两个 owner，也迫使每个新领域重复实现通用工作状态。

## 决定

1. Platform Data Intake 是上传、SourceAsset revision、通用画像、显式映射、参数和
   Dataset Release 的唯一 owner。
2. Domain package 声明 `DataRequirementContract`，并实现业务 validator/normalizer；
   它读取授权 release/row stream，不扫描 Workspace，不保存第二份 source/mapping 状态。
3. Platform Work State 保存 Task 范围的 component revision、operation、dependency、
   stale/readiness、blocking input 和 deliverable 元数据。
4. Domain package 拥有 component payload schema、业务字段、算法、校验和 invalidation
   规则；大型 payload 通过 Dataset/Artifact/Domain Resource 引用。
5. Work State 不是 Thread、Memory、Workflow Engine 或 Blackboard，也不调度 Agent。
6. 分析只验证当前问题的最小必要数据，不再要求一个全局单体 Dataset。

## 否决方案

- 把 Network Case 直接升格为平台对象；
- 继续让每个 Data Agent 遍历目录并建立自己的映射数据库；
- 把所有领域 payload 放入 Platform JSONB；
- 用长 Agent 消息交换完整数据。

## 后果与验证

供应链必须删除 case_sources、mapping lifecycle 和通用 operation store；Platform 必须提供
可授权、幂等、并发安全的 Work State API。第二个非供应链案例不得修改平台领域分支。

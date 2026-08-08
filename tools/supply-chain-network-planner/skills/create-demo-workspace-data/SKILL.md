---
name: create-demo-workspace-data
description: 仅在用户明确要求 mock、demo、示例或教程数据时，把经过校验的印尼教程数据放入 Workspace。
---

# 创建教程示例数据

示例数据不是失败回退。

1. 先确认用户明确要求使用 mock、demo、示例或教程数据。
2. 调用 Demo 工具一次，把经过校验的 fixture 写入授权 Workspace。不得使用 shell `source`，不得因 Workspace 为空自动执行。
3. 使用 `refresh_case_sources`、`inspect_case_sources` 和映射工具处理生成的文件，流程与真实用户文件完全一致。
4. 保留 `synthetic_demo` 分类、来源、生成器版本和校验结果。

基础 fixture 包含 50 个需求城市、5 个中心仓、6 个 cross-docking 仓和完整现有仓到需求城市报价；当前覆盖和候选仓属于独立扩展数据。不得自动加载扩展数据、修改生成文件或把示例结果描述成真实业务现状。

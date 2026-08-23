# 进阶：交付与 UI

> **适合谁**：需要把报告或地图作为最终结果显示在 Web 的开发者
> **预计时间**：10 分钟
> **前置条件**：Tool 已返回稳定 typed result，且知道最终结果与中间 Resource 的区别
> **完成结果**：manifest 精确声明 producer，Web 只展示已验证交付

## 只有最终结果才声明 delivery

普通 Tool 文本、结构化中间结果、MCP Resource 和 Workspace 临时文件都不会自动成为 Artifact。
只有用户明确需要保留/展示的报告或地图，才在 `[[deliveries]]` 声明精确 `(server, tool)` producer。

当前 kind：

- `workspace_artifact`：最终文件；必须用受限 JSON Schema 或单行 Markdown marker 验证内容；
- `inline_geojson_map_card`：最终交互地图；使用固定媒体类型与平台版本化 map verifier。

仓网的当前四项 producer 见[精确声明浏览器交付](../warehouse-copilot.md#4-精确声明浏览器交付)。

## UI 不推断领域语义

Platform 只消费经过验证的 delivery envelope；不根据模型文字、Tool 显示名、文件扩展名或领域字段
猜类型。Browser 收到有界 DTO，不读取 server absolute path、内部 descriptor 或 Resource 内容。

地图的几何与业务结果由领域 Tool 生成，Maps Tool 只把 exact GeoJSON ref 变成卡片规格；纯样式
修订复用几何，不能暗中改变业务结论。

## 验收

同时检查 Tool Item、structured result、delivery producer、浏览器卡片和刷新后的 Task history。
只看最终回答或一张截图都不足以证明 producer/持久化正确。

成功信号：同一精确 Tool Item 产生通过 verifier 的交付；刷新后仍可解析；失败结果不会出现成功
卡片。若 Web 缺包或缺卡片，见
[Web 中找不到或没有真实调用](../testing-and-troubleshooting.md#web-中找不到或没有真实调用)。

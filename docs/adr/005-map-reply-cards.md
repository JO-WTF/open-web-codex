# ADR-005: 地图回复卡片实现方案

**状态**: 已提议  
**日期**: 2026-07-21  
**关联**: ADR-004, `docs/development-plan.md` 第 13 节

## 背景

地理相关问题需要在自然语言回复中嵌入可交互地图，例如经纬度查询、位置说明、两地路线或距离、行政边界、以及地理数据可视化。用户可视化的数据可能达到数 MB，不能要求模型逐字输出完整 GeoJSON；同时项目要求保持 Codex Runtime 与 Web 平台边界，避免把地图业务逻辑散落到 `codex/`。

## 参考实现提炼

参考 `JO-WTF/agent-framework` 后，首版实现吸收以下做法，并按本项目的多用户与合同边界做加强：

- **轻量 widget 标记 + 后端存储完整卡片。** 参考实现的 `render_map_card` 工具只把 `widget_type=map`、`id` 和 `use_stored_card` 返回给模型，完整 points/lines/GeoJSON 留在 Web session。本文档保留这个低 token 成本思路，但把 session state 升级为平台 Artifact 与 Task event 投影。
- **工具结果引用避免大 payload 进模型。** 参考实现把行政边界、路线等 GeoJSON 存为工具结果引用，再提示后续地图工具使用引用。本文档采用同类 `input_ref`/Artifact 引用链路，要求大 GeoJSON 只在服务端 builder 与 Artifact store 之间流动。
- **服务端先切块再渲染。** 参考实现先把 assistant reply 解析为有序 text/widget blocks，再由前端渲染占位。本文档沿用“消息正文顺序 + card slot”的设计，但解析器必须在平台 event normalizer 中运行，并输出稳定 `placement`。
- **地理数据归一化和上限。** 参考实现对 lat/lng 范围、点数、线坐标数和 zoom 做校验。本文档把这些校验前移到 `map-card-builder`，并补充 bbox、schema、样式白名单、hash 和 Artifact 大小限制。
- **地图资源控制。** 参考实现限制活跃 Mapbox 实例并挂起旧地图。本文档把 active map budget、懒加载、重新激活和 worker 解析列为浏览器验收项，避免多卡片消息拖垮页面。
- **专门地理工具输入。** 参考实现把地理编码、行政边界、路线、距离和 POI 搜索拆成工具。本文档不要求首版内置所有外部 API，但 builder 输入接口必须能接收这些工具或 MCP 结果，并保留审计、Secret 隔离和失败降级。

不能直接照搬的部分：参考实现是单会话内存/文件状态，卡片通过 session `map_cards` hydration；本项目必须使用组织、Task、Run、Profile、Artifact 和权限链路，不能把卡片存在浏览器可信状态或未授权 session 缓存中。

## 决策

地图卡片以 **Web 平台 Artifact + 结构化回复标记** 实现。Codex 仍输出自然语言回答和一个很小的卡片占位标记，平台负责解析占位、生成或接收 GeoJSON、持久化 Artifact、发送浏览器 DTO，并由浏览器使用 Mapbox GL 渲染。

### 1. 数据流

```text
User prompt
  -> Codex Thread / Turn
  -> assistant text with compact card marker
  -> platform event normalizer parses marker
  -> map-card-builder creates/validates GeoJSON + style
  -> artifact store persists payload under Task/Run/Profile ownership
  -> task event stream emits reply_card.v1 DTO
  -> browser renders text and card slots in original order
  -> Mapbox GL fetches authorized Artifact URL and renders inline/fullscreen map
```

占位标记只描述意图和小型参数，不承载大型几何数据。大型 GeoJSON 只能通过 Artifact 引用进入浏览器。

### 2. 回复内嵌标记

首版使用 fenced block，便于提示模型稳定生成并便于流式文本解析：

````markdown
```open-web-card map.v1
{
  "title": "北京到上海路线",
  "intent": "route",
  "input_ref": "artifact_or_builder_input_id",
  "fallback_text": "已生成北京到上海的路线地图。"
}
```
````

解析规则：

- 只接受 `open-web-card` 语言标签和受支持的 `kind.version`。
- fenced block 最大 16 KB；超过上限按普通文本处理并记录兼容指标。
- JSON 只允许 schema 中声明的字段；未知字段保留在诊断日志中，不发送给浏览器。
- 解析成功后，浏览器消息正文中的原始 fenced block 被 card slot 替代；解析失败时显示 `fallback_text` 或原文。

### 3. 平台 DTO

平台对浏览器发送统一外层卡片 DTO：

```json
{
  "type": "reply_card.v1",
  "id": "card_...",
  "task_id": "task_...",
  "run_id": "run_...",
  "message_id": "msg_...",
  "sequence": 42,
  "placement": { "message_part_index": 3 },
  "kind": "map.v1",
  "title": "北京到上海路线",
  "summary": "路线、距离和关键点",
  "status": "ready",
  "artifact_id": "artifact_...",
  "fallback_text": "已生成北京到上海的路线地图。"
}
```

`map.v1` Artifact metadata 包含：

```json
{
  "schema_version": "map.v1",
  "geojson_artifact_id": "artifact_...",
  "geojson_sha256": "...",
  "geojson_bytes": 734218,
  "bbox": [116.39, 31.23, 121.47, 39.90],
  "viewport": { "fit_bounds": true, "padding": 32 },
  "layers": [
    {
      "id": "route",
      "source": "geojson",
      "geometry": "LineString",
      "paint": { "line-color": "#2563eb", "line-width": 4 }
    }
  ],
  "legend": [{ "label": "路线", "color": "#2563eb" }]
}
```

### 4. 服务端组件

实现分为四个 Web 平台组件，不在首版修改 `codex/`：

1. **Card marker parser**：在 Run event normalizer 中解析 assistant text，输出 card slot 和 builder job。
2. **Map card builder**：验证坐标、bbox、FeatureCollection、样式白名单和大小限制；必要时从工具结果或平台地理服务生成 GeoJSON。
3. **Artifact repository**：保存 GeoJSON、metadata、hash、MIME、大小、owner、Task/Run 绑定、保留期和审计事件。
4. **Card event projector**：把 `loading`、`ready`、`error` 作为单调 Task event 发给浏览器，确保重连后可恢复同一张卡片状态。
5. **Tool-result resolver**：支持 `input_ref` 指向先前工具/MCP 输出，按 Task/Run/Profile 归属读取大型 GeoJSON，再交给 builder；禁止模型把引用扩展为正文。

### 5. 浏览器组件

浏览器实现三个层次：

1. **Message renderer**：按 message part 顺序渲染 Markdown 和 card slot，支持一条回复中的多张卡片。
2. **MapCard**：获取授权 Artifact URL，校验 metadata，初始化 Mapbox GL，绘制点/线/面，支持 bbox fit、legend、tooltip、错误占位和下载原始 GeoJSON。
3. **Fullscreen map dialog**：复用同一 metadata，提供全屏查看、键盘退出、移动端布局和无障碍标签。
4. **Map resource manager**：限制同屏活跃 Mapbox 实例数量，离屏或超预算地图进入 suspended 状态，用户点击后重新激活。

### 6. 大数据策略

- 默认 GeoJSON Artifact 上限由 capability limits 下发，首版建议软上限 5 MB、硬上限 20 MB。
- 超过软上限时 builder 尝试 simplify、属性裁剪或生成概要图层。
- 超过硬上限时卡片进入 `error`，保留 Artifact 下载入口或提示用户拆分数据。
- 浏览器解析放入 worker；主线程只接收已裁剪 metadata 和渲染必要数据。
- 后续可加入矢量瓦片或 PMTiles，但不作为首版门槛。

### 7. 安全与权限

- Artifact URL 必须通过平台 API 鉴权，不能是本地路径或永久公开 URL。
- GeoJSON properties 作为不可信输入处理，tooltip/legend 文本必须转义。
- Mapbox token 只使用公开受限 token，不能复用服务端 Secret。
- 卡片 payload 不包含 app-server request ID、Profile Home、Workspace path、Provider Secret 或原始协议 payload。
- 所有 card create/read/download/fullscreen 操作都绑定 Task 可见性权限并写审计。

### 8. Codex 收敛策略

首版不改 `codex/`。如果后续证明现有 message/event 无法稳定携带 card marker，再按以下顺序处理：

1. 优先消费官方 app-server 已有消息内容或 artifact/reference 类型。
2. 其次在生成 protocol/manifest 中增加最小卡片引用类型。
3. 最后才增加小型 Runtime seam，并按 patch map 分类、运行 upstream/customization 状态脚本和 scoped tests。

## 实施顺序

1. 定义 `reply_card.v1`、`map.v1` JSON Schema、TypeScript/Rust DTO 和 feature policy gate。
2. 增加 ordered block parser + Fake builder，把静态点/线/面 Fixture 渲染到浏览器。
3. 增加 tool-result resolver 和 Artifact repository，验证 `input_ref` 可把边界/路线 GeoJSON 转成 Artifact-backed map card。
4. 接入 Mapbox GL inline/fullscreen 组件、active map budget、suspended/reactivate、移动端和错误状态。
5. 增加真实 app-server smoke：模型回复文本中混排普通段落和 map marker，平台生成 Artifact，浏览器渲染卡片。
6. 扩展 builder：路线、边界、外部地理工具/MCP 输入、大数据 simplify/worker fallback。

## 验收标准

- 地理问题的回复可以在任意段落前后插入地图卡片，刷新和断线重连后顺序与状态不变。
- 5 MB 级 GeoJSON 不出现在模型输出正文或 WebSocket 普通文本事件中，模型只看到 card marker 或 `input_ref`。
- 点、线、面和 FeatureCollection 按样式白名单渲染，并支持全屏查看。
- 另一用户或无权限项目成员无法读取卡片 metadata、GeoJSON Artifact 或下载 URL。
- 不修改 `codex/` 即可完成首版；若必须修改，差异已分类且有合同/Smoke/patch map 证据。

## 替代方案

- **让 LLM 直接输出完整 GeoJSON**：实现简单，但大数据慢、费用高、上下文污染严重，并且容易产生无效 JSON。
- **浏览器从第三方地理 API 直接取数**：降低服务端工作量，但会暴露 token、绕过平台审计和权限，并破坏可恢复性。
- **在 Codex Runtime 内建地图卡片语义**：可获得更原生的协议形态，但扩大上游同步冲突面，不符合当前产品边界。

## 影响

- 正向：地图可视化成为可审计、可恢复、可门控的平台能力，且支持大型 GeoJSON。
- 负向：需要新增 Artifact 存储、builder job、浏览器 Mapbox 依赖和安全测试矩阵。

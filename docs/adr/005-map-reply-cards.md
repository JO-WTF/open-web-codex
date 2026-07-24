# ADR-005: 地图回复卡片与 MCP Resource 引用

**状态**: 已接受

**日期**: 2026-07-24

**关联**: ADR-004, `docs/development-plan.md` 第 13 节

## 背景

地理回答需要在自然语言回复的准确位置嵌入一张或多张交互地图。地图数据可能是数 MB 的 GeoJSON，不能要求模型复制到卡片参数或 assistant 文本。地图还需要明确的 camera/fit viewport，以及点、线、面的颜色、透明度、尺寸、描边和虚实线样式。

实现必须复用官方 Codex 的 Thread/Turn、MCP Tool 和 MCP Resource 语义，不在 Runtime 中增加 Web 卡片协议。

## 决策

地图采用 `open-web-card` / `map.v2`：

1. 数据 Tool 把 GeoJSON 保存为 MCP Resource，并在结构化 `data_ref` 中返回原始 MCP server ID 与标准 `resource_link.uri` 对应的 URI。完整 `data_ref` 可原样传给卡片；其 `server`、`uri` 也可原样供 MCP `resources/read` 使用，不再定义第二套本地资源身份。
2. 同一 Run、同一 Thread 中后续的 `create_map_card` 直接复用较早完成的 Tool 产生的 `data_ref`，也可以携带小型 inline GeoJSON。卡片没有 16 KiB 专用限制。
3. Platform Server 只从 `structuredContent` 识别卡片，通过 MCP `outputSchema` 之外的第二层合同验证后投影浏览器 DTO。Server 不读取 `content[].text` 或 assistant Markdown 来猜测卡片。
4. Platform 使用官方 `mcpServer/resource/read` 延迟读取 Resource，把来源 server/URI 隐藏在授权 Artifact 后面，并缓存内容。
5. 浏览器保留既有执行组语义：Reasoning、Tool、审批、命令和中间 assistant 消息继续使用专用样式并折叠；reply card 与最终 assistant 回复按 Codex Turn item 顺序展示。一次回复可以出现多张卡片，未来天气、图表等卡片复用同一个 reply-card 分派框架。

只有 `map.v2` 结构化合同是卡片输入；没有文本标记或其他地图载荷分支。

## MCP 合同

数据 Tool 结果包含：

```json
{
  "content": [
    {
      "type": "text",
      "text": "Geocoded 3 addresses; copy data_ref into a map card or use its server and uri to read the GeoJSON."
    },
    {
      "type": "resource_link",
      "name": "map-data-8a4c...",
      "uri": "maps-data://geojson/map-data-8a4c...",
      "mimeType": "application/geo+json",
      "size": 734218
    }
  ],
  "structuredContent": {
    "provider": "mapbox",
    "summary": "Geocoded 3 addresses; copy data_ref into a map card or use its server and uri to read the GeoJSON.",
    "feature_count": 3,
    "data_ref": {
      "type": "mcp_resource",
      "server": "map_utils",
      "uri": "maps-data://geojson/map-data-8a4c...",
      "format": "geojson"
    }
  }
}
```

`create_map_card.structuredContent`：

```json
{
  "type": "open-web-card",
  "kind": "map.v2",
  "card": {
    "title": "北京到上海路线",
    "intent": "route",
    "status": "ready",
    "summary": "路线概览",
    "viewport": {
      "mode": "fit",
      "padding": 48,
      "max_zoom": 14
    },
    "sources": [
      {
        "id": "route-data",
        "data": {
          "type": "mcp_resource",
          "server": "map_utils",
          "uri": "maps-data://geojson/map-data-8a4c...",
          "format": "geojson"
        }
      }
    ],
    "layers": [
      {
        "id": "route",
        "source": "route-data",
        "geometry": "line",
        "style": {
          "color": "#2563eb",
          "opacity": 0.9,
          "width": 5,
          "dash": [2, 1],
          "cap": "round",
          "join": "round"
        }
      }
    ]
  }
}
```

合同规则：

- Tool 声明 `outputSchema`，MCP Server 在发出结果前验证字段和跨字段不变量。
- Platform 仅接受 `type=open-web-card`、`kind=map.v2` 和白名单字段。
- `resource_link.uri`、`data_ref.uri` 与 `create_map_card.sources[].data.uri` 是同一规范 MCP Resource URI；`data_ref.server` 是原始 MCP 配置键 `map_utils`。调用 `read_mcp_resource` 时必须原样传递这两个字段，不能把模型可见命名空间 `mcp__map_utils` 当作 server；`name` 是逻辑资源名，`title` 才是人类可读标题。
- 卡片的 server+URI 必须唯一解析到同一 Run、同一 Thread 中另一条较早完成的 MCP Tool item。跨 Run、跨 Thread、后向引用、缺失、冲突或同 item 自引用不会提升为卡片。
- inline 数据必须是 GeoJSON root；大型数据使用 MCP Resource。不存在卡片专用字节上限，平台仍保留通用请求、事件和 Resource 内存安全边界。
- Artifact 注册和 Resource 解析使用独立数据库 savepoint；即使卡片投影失败，也必须持久化并广播原始 MCP Tool 的 completed 生命周期，不能让浏览器永久停在 `inProgress`。Platform Store 的 build script 递归跟踪 migrations 目录，保证新增 migration 被下一次 Rust 构建嵌入。
- viewport 是互斥联合：`fit` 负责 padding/min/max zoom，`camera` 负责 center/zoom/bearing/pitch。
- layer 是 point、line 或 polygon 的判别联合，样式字段使用数值范围和颜色白名单验证。
- `content` 是给模型看的简短摘要和标准 ResourceLink，不复制卡片 JSON。assistant 文本只作为 Markdown。

## Platform Artifact

`reply_artifacts` 保存：

- organization、Run、Thread、Turn 和 producer item 所有权；
- 内部 MCP server/URI、MIME 和预期大小；
- 延迟读取后的内容、SHA-256 和状态。

浏览器加载数据时只看到 `/api/runs/{run}/artifacts/{artifact}`。接口重新检查组织、Run 发起人或管理员权限，然后用绑定 Thread 和 Workspace 调用 Adapter 的类型化 `read_mcp_resource`。公开 ResourceLink 投影会移除内部 Resource URI，Artifact 响应也不包含来源 URI、Workspace path 或 Runtime request ID；普通 Tool 事件仍可显示其逻辑 MCP server/tool 名。

Resource 缓存有通用的 128 MiB 单资源内存安全边界；它不是 `map.v2` 合同大小限制。更大数据需要新增 PMTiles/MVT 流式 source 类型，而不是扩大 assistant 或 WebSocket payload。

## 回复位置与多卡片

Codex Thread item 顺序是位置事实，但不改变现有消息分组语义。Web 继续把
Reasoning、Tool、审批、命令和中间 assistant 消息放入可折叠执行组；带卡片的
`mcpToolCall` 仍是 Tool item，`replyCard` 只是该 item 附带的额外展示块：

```text
execution_group(reasoning / tools / intermediate assistant)
reply_card(map.v2)
final assistant Markdown
reply_card(map.v2)
```

卡片和最终 assistant 的相对位置由原始 item 索引决定；卡片不能替换其来源 Tool，
也不能改变执行组的 Tool/message 计数。renderer 根据 `replyCard.kind` 分派专用组件，
所以未来卡片类型不需要修改 Thread/Turn 协议。

## 地图渲染

- 每个 source 对应独立 Mapbox GeoJSON source；layer 按引用绘制。
- `fit` 在 Mapbox `load` 后执行，并在容器首次获得非零尺寸后再次执行，避免初始隐藏布局导致中心落到 0,0。
- `camera` 直接应用 center、zoom、bearing 和 pitch。
- 点支持颜色、透明度、半径和描边；线支持颜色、透明度、宽度、dash、cap、join；面支持填充和描边样式。
- GeoJSON properties 作为不可信文本，不能注入 HTML。
- Mapbox 只接收受站点来源限制的公开 `pk.` Token；地理服务 Secret 不进入浏览器。

## Codex 收敛

不修改 `codex/`。官方 app-server 已提供：

- `mcpToolCall.result.structuredContent` 和标准 MCP content blocks；
- `mcpServer/resource/read`；
- 持久 Thread/Turn item 顺序。

Web Adapter 只增加对官方 Resource read 方法的类型化调用。卡片验证、Artifact 权限和浏览器 DTO 均属于 Web 平台。

## 验收标准

- 非 `map.v2` structuredContent 和文本 JSON 不会渲染为卡片。
- 大 GeoJSON 不出现在 assistant 正文或卡片 `structuredContent`，只通过 MCP Resource URI 和授权 Artifact 传输。
- MCP Resource 引用不能跨 Run、跨 Thread、跨组织、引用当前 create-card item 或引用尚未完成的后续 item；允许跨 Turn 引用较早完成的 Resource。
- camera zoom 精确生效，fit 覆盖所有已加载 source 且不会停在 0,0。
- 点、线、面样式按合同生效。
- 执行组、reply card 和最终 assistant 的相对顺序在实时显示、刷新和 Thread 历史恢复后保持一致。
- 一次回复可以包含多张地图卡片，并为其他 reply-card kind 保留分派位置。

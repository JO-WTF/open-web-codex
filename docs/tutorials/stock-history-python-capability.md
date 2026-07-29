# 股票历史查询：在 Web 创建 Python MCP 与 Skill

本篇从 Web 创建一个可运行的 Python capability package。它包含：

- 一个 MCP Server；
- `lookup_stock` 和 `get_price_history` 两个 Tool；
- 一个告诉 Codex 正确调用顺序的 Skill。

用户最终只需询问某只股票和日期范围，Codex 就会先解析股票代码，再查询历史记录。
预计用时 15–25 分钟。

开始前确认平台已按[本地运行手册](../mvp-runbook.md)启动，并且 Web 左侧至少有一个
Workspace。

## 完成后的链路

```mermaid
flowchart LR
    U["用户问题"] --> S["stock-history Skill"]
    S --> L["lookup_stock"]
    L --> I["名称、交易所、股票代码"]
    I --> H["get_price_history"]
    H --> R["指定日期范围的历史记录"]
    R --> A["带来源的回答"]
```

正确结果不是模型直接写出价格，而是 Thread 中能看到两个真实 MCP Tool 调用，并且
第二个调用使用第一个调用返回的股票代码。

## 1. 先选择数据来源

准备两个返回 JSON 的 HTTP 接口：

| 接口 | 输入 | 至少需要返回 |
| --- | --- | --- |
| 股票基础信息 | 股票或公司名称 | 名称、股票代码、可选交易所 |
| 历史价格 | 股票代码、开始日期、结束日期 | 对应日期范围的记录数组 |

当前新手切片只支持 Python 标准库和不需要 Secret 的 HTTP 接口。需要 API Key、
OAuth、Cookie 或企业凭据时，不要把凭据写进 Python、Skill 或 Prompt；等平台提供
类型化 Secret 绑定后再接入。

还要确认：

- 网站允许这种 API 使用方式；
- 响应是稳定 JSON，而不是需要浏览器执行脚本的 HTML；
- 日期范围有明确上限；
- 返回中能保留数据来源；
- 失败时接口返回可判断的错误，而不是空成功。

## 2. 打开 Python capability 编辑器

在 Web 中：

1. 打开右侧 **Agents**；
2. 进入 **Agent Studio**；
3. 点击 **New Python capability**；
4. 选择要保存 package 的 Workspace。

编辑器已经预填股票历史案例。基础字段可先保留：

| 字段 | 示例值 |
| --- | --- |
| Package ID | `stock-history` |
| Version | `1.0.0` |
| MCP server name | `stock_data` |
| Display name | `Stock history` |
| Skill name | `stock-history` |

Package ID、MCP Server 名和 Tool 名使用稳定的小写标识，不要写本地路径、命令或
Provider 名。

## 3. 检查两个 Tool 合同

**Tools** 默认包含：

```json
[
  {
    "name": "lookup_stock",
    "description": "Resolve a stock name to its exchange and ticker symbol.",
    "input_schema": {
      "type": "object",
      "properties": {
        "name": { "type": "string" }
      },
      "required": ["name"],
      "additionalProperties": false
    }
  },
  {
    "name": "get_price_history",
    "description": "Fetch daily price history for a ticker and bounded date range.",
    "input_schema": {
      "type": "object",
      "properties": {
        "ticker": { "type": "string" },
        "start_date": { "type": "string" },
        "end_date": { "type": "string" }
      },
      "required": ["ticker", "start_date", "end_date"],
      "additionalProperties": false
    }
  }
]
```

不要把两个步骤合成一个含糊的 `run` Tool。分开的类型化合同让 Runtime、用户和日志
都能看清股票代码来自哪里，以及历史查询实际用了什么输入。

## 4. 修改 Python 实现

在 **Python implementation** 中先替换：

```python
BASE_INFO_URL = "https://example.com/api/stocks/search"
HISTORY_URL = "https://example.com/api/stocks/history"
```

然后按真实网站响应修改两个函数中的字段映射：

```python
def lookup_stock(arguments):
    ...
    return {
        "name": data["name"],
        "ticker": data["ticker"],
        "exchange": data.get("exchange"),
        "source": BASE_INFO_URL,
    }

def get_price_history(arguments):
    ...
    return {
        "ticker": arguments["ticker"],
        "start_date": arguments["start_date"],
        "end_date": arguments["end_date"],
        "records": data["records"],
        "source": HISTORY_URL,
    }
```

每个函数接收一个 `arguments` 字典，并返回可编码为 JSON 的数据。保留 `source`、
请求股票代码和日期范围，避免最终回答失去来源或把不同口径混在一起。

当前编辑器不允许用户控制 package 路径、launcher、进程环境或 MCP 协议。平台会生成
这些部分，并在隔离环境中探测 Server。

## 5. 写清 Skill 调用顺序

**Skill instructions** 填写：

```text
当用户询问股票详情或历史价格时，先调用 stock_data.lookup_stock，并把用户提供的
公司或股票名称作为 name。只使用返回的 ticker 调用 stock_data.get_price_history。
历史查询必须使用用户指定的开始和结束日期；用户没有提供日期范围时先询问，不要猜测。
最终回答同时报告两个 Tool 返回的 source，不得编造缺失记录。
```

Skill 只描述何时使用能力和调用方法。它不会执行 HTTP 请求，也不能扩大 MCP 权限。

## 6. 验证和测试

先点击 **Validate**。成功提示应包含：

```text
MCP startup and Tool discovery passed for: lookup_stock, get_price_history.
```

这一步证明生成的 MCP Server 能启动，并且 Runtime 协议可以列出两个 Tool；它不证明
真实网站一定能返回正确数据。

然后测试：

1. **Tool to test** 填 `lookup_stock`；
2. **Arguments** 填一个真实名称，例如 `{"name":"示例公司"}`；
3. 点击 **Test Tool**；
4. 确认结果包含非空 `ticker` 和正确 `source`；
5. 再把 Tool 改为 `get_price_history`；
6. 使用上一步返回的 ticker 和一个短日期范围测试；
7. 确认 records 属于请求范围。

任一 Tool 测试失败都不要发布。先按页面错误检查 URL、字段映射、超时、JSON 格式和
日期参数。

## 7. 发布并启动测试 Thread

点击 **Publish package**。成功后页面应显示：

```text
stock-history@1.0.0 published
Includes MCP stock_data and Skill $stock-history.
```

点击 **Start test Thread**，发送：

```text
查询示例公司从 2026-07-01 到 2026-07-05 的股票历史详情，并说明股票代码、交易所和数据来源。
```

把公司名称和日期改成两个网站真实支持的值。

## 8. 成功检查表

- [ ] `Validate` 证明 MCP 启动和 Tool discovery 成功；
- [ ] 两个 Tool 分别用真实参数测试成功；
- [ ] package、MCP 和 Skill 使用预期 ID；
- [ ] 新 Thread 先调用 `lookup_stock`；
- [ ] `get_price_history` 使用前一步返回的 ticker；
- [ ] 日期范围与用户问题一致；
- [ ] 最终回答保留两个来源；
- [ ] 页面中没有 Secret、本地路径或内部 Runtime 请求 ID。

## 9. 常见问题

### `NameError`、`KeyError` 或字段为空

Python 字段映射与网站实际 JSON 不一致。查看 Tool 测试结果，修改实现，不要在 Skill
中让模型猜字段。

### MCP 启动成功，但 Tool 调用失败

启动探测只证明 Server 和协议正确。网络、网站状态、HTTP 错误或响应格式仍可能在
调用时失败，应保留原始错误原因并修复对应接口。

### Thread 没有调用 Skill 或 Tool

必须从发布成功后的 **Start test Thread** 创建新 Thread。已经存在的 Thread 不会因
后来发布 package 而静默改变有效能力。

### 版本已经存在

发布版本不可变。修改代码、Tool Schema 或 Skill 后创建 `1.0.1`，不要覆盖
`1.0.0`，也不要增加读取旧版本的 fallback。

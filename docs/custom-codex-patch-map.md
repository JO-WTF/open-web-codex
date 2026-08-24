# Custom Codex patch map

本文是 `codex/` 当前本地差异的唯一人工库存。它只记录仍须保留的
Runtime seam；生成 schema、TypeScript、锁文件、fixture 和 snapshot 都是所属 seam
的 followers，不是独立功能。

## 当前快照

- 记录的 comparison snapshot：integrated 与当时 official upstream 均为
  `76d98a771e6cd44a79a3ab895a9f7c49d27d6deb`
- 比较对象：`HEAD:codex` 对 `codex-upstream/main`
- 已分类差异：116 个 local-only（26 added、90 modified、0 missing），0 upstream-only、0 diverged
- 状态：`runtime-validated-product-e2e-pending`

这个状态只说明代码已经按下列 seam 收敛；它不宣称完整 Runtime、app-server、Web 或真实
Provider E2E 已完成。实时计数以
`scripts/codex-upstream-status.sh` 和 `scripts/codex-customization-status.sh` 为准，机器
快照由 `.sync/codex-customization-inventory.json` 保存。

## 保留 seam

| ID | 最小 owner | 必要性 | 回放顺序 |
| --- | --- | --- | --- |
| `provider-chat-wire-adapter` | `codex-api` Chat endpoint/SSE/translation；`core/src/client{,/chat}.rs` 的窄分派 | 第三方 OpenAI-compatible Chat Completions wire 尚无上游等价适配。 | 1 |
| `provider-configured-model-capabilities` | `model-provider-info`、`model-provider/provider.rs`、`models-manager`、remote thread config、Core capability consumer | 配置 Provider 的模型目录和 function-tool 能力必须是显式 typed 事实。 | 2 |
| `provider-app-server-catalog-api` | app-server protocol/catalog handler、bounded catalog client、route-aware login client | Browser/Platform 需要官方 app-server 后面的 Provider 列表和 fresh catalog，而不直接持有 URL 或凭据。 | 3 |
| `provider-error-redaction` | shared API bridge 与 Bedrock error mapper | Provider 401 的 secret/log 边界不能依赖上游错误文本。 | 4 |
| `plugin-mcp-tool-exposure` | Plugin MCP config policy 与 loader | 已选 Plugin server 必须能声明 Runtime 原生的 Tool exposure omission。 | 5 |
| `managed-child-role-projection-resume` | Role projection、cold resume、upstream V2 AgentControl attachment | package-owned child Role 的受信 MCP/Skill/指令投影在当前仓网 V1 cold resume 尚无上游等价。 | 6 |
| `stdio-mcp-liveness` | `rmcp-client` stdio process/transport lifecycle | stdio MCP 自然退出必须以 Runtime-owned typed liveness 结束连接，不能让 app-server 状态投影继续显示 Connected。 | 7 |

### `provider-chat-wire-adapter`

**Owner paths.** `codex-rs/codex-api/src/{chat_translate.rs,chat_translate_history.rs,chat_translate_tests.rs,endpoint/chat.rs,sse/{chat.rs,chat_state.rs,chat_wire.rs}}`；仅在
`codex-rs/core/src/{client.rs,client/chat.rs}` 保留 `WireApi::Chat` 分派和 Runtime event
attachment。

**Typed contract.** 适配器把 canonical Responses request/history 映射到 Chat request，并把
Chat SSE 映射回 `ResponseEvent`。已完成的 client `ToolSearch` call/output 在 canonical
history 中提供 exact namespace/name/schema 的 request-scoped Chat reverse map；不改写
`Prompt.tools`、不建立 Platform cache。Chat 返回 Tool 必须先通过该 map，再由 Core ToolRouter
执行最终身份、权限和注册表检查。`FunctionCallOutput.call_id=None` 是 typed unsupported request，
不能伪造、跳过或 panic。

**为什么保留。** 上游没有配置 Provider 的 Chat Completions wire transport；仅靠配置无法保留
native tool/history/SSE 语义。

**验证门。** `./scripts/test-codex.sh -p codex-api`；Core Chat/client 的 typed history、ToolSearch、
Tool group 与 stream regression。

**退出条件。** 上游提供等价的第三方 Chat transport，或配置 Provider 能以 native Responses
契约表达同一能力。

### `provider-configured-model-capabilities`

**Owner paths.** `codex-rs/model-provider-info/src/{lib.rs,model_provider_info_tests.rs}`；
`model-provider/src/provider.rs`；`models-manager/src/{manager.rs,manager_tests.rs,model_info.rs}`；
`config/src/thread_config/**`；`core/src/tools/spec_plan.rs` 仅消费 typed capability。

**Typed contract.** `WireApi::Chat`、`supports_function_tools` 和
`ProviderModelConfig` 是 `ModelProviderInfo` 的显式字段。Configured Provider 仅按 exact
`model_id` 把 `info.models` 传给 static、remote、无 cache 和有 cache 四个
`ModelsManager` 构造路径；未配置的 model capability 为 false，绝不按 Provider 名、模型名或
描述猜测。Configured Chat 同时降权 `web_search=false`，因为当前 Chat translator 不编码 hosted
web search；Responses/default 行为不变。Bedrock 明确 function-tools=true。

**为什么保留。** 当前上游没有这一份 Provider-owned typed configuration 到运行时模型信息的
闭环，缺失时会让 Tool planning 错报能力。

**验证门。** `./scripts/test-codex.sh -p codex-model-provider-info`、
`-p codex-model-provider`、`-p codex-models-manager`；Core tool-plan 的 function-tool
typed rejection regression。

**退出条件。** 上游提供同等的 configured Provider model catalog、exact capability merge 与
safe-default semantics。

### `provider-app-server-catalog-api`

**Owner paths.** `codex-rs/app-server-protocol/src/protocol/{common.rs,v2/{model.rs,shared.rs}}`；
`app-server/src/{message_processor.rs,request_processors/{catalog_processor.rs,config_processor.rs}}`；
`codex-api/src/{endpoint/{models.rs,session.rs},lib.rs}`；
`login/src/auth/default_client.rs`；`model-provider/src/{models_endpoint.rs,lib.rs}`。

**Typed contract.** `modelProvider/list` 只由 latest Runtime config registry 解析 Provider，
按配置顺序返回每个 `models` 和 `model_count`，并返回 typed nullable `currentModelId`；它不做 fresh
fetch、不写 cache/selection，也不投影 auth/query/header。`modelProvider/models/list` 的请求者不能
提交 URL 或凭据，fresh catalog 严格接受 bounded rich/OpenAI
`/models`，以 body-free `NotFound`、`RateLimited`、`Upstream`、`InvalidJson`、timeout 等分类
返回；不切换 current Provider，不写 `ModelsManager`/cache，不改变 Thread/Turn。Provider list 和
capabilities 投影显式 `supportsFunctionTools`，并保留 typed
`providerFunctionToolsUnsupported`。

**为什么保留。** Platform 不能安全地绕过 app-server 直接做 Provider discovery，且上游尚未
提供这组稳定 catalog contract。

**验证门。** `./scripts/test-codex.sh -p codex-login default_client`、
`-p codex-model-provider fresh_catalog`、`-p codex-app-server model_provider_list`、
`-p codex-app-server model_provider_models_list`、`-p codex-app-server-protocol`；重新生成并检查
protocol Schema/TypeScript，以及 real app-server smoke。

**退出条件。** 上游提供等价的 Provider-scoped fresh catalog、capability API、typed error 与
preserving no-request-logging route client。

### `provider-error-redaction`

**Owner paths.** `codex-rs/codex-api/src/{api_bridge.rs,api_bridge_tests.rs}`；
`model-provider/src/amazon_bedrock/{error.rs,error_tests.rs}`。

**Typed contract.** 任意 Provider 401 在 shared mapper 中变成固定安全文案、empty body 和
`url=None`；Display、Debug 和 ErrorEvent 都不得含原 body、URL 或凭据。Bedrock 只在 shared
mapper 前检查 raw typed `ApiError` 的 `401 + Signature expired` marker，随后仅以已脱敏的
`CodexErr` 替换为上游静态指导，同时保留 retry delay 和安全 request id。其他 401 与非 401
signature 不特判。

**为什么保留。** Provider credentials 与错误 body 的日志边界是本产品的硬安全门；删去 Bedrock
分支会丢失上游静态修复指导。

**验证门。** `./scripts/test-codex.sh -p codex-api`、
`-p codex-model-provider amazon_bedrock::error_tests`，以及 Core client error regression。

**退出条件。** 上游 shared 401 mapper 同时保证无 secret 输出并保留同等 Bedrock 静态指导。

### `plugin-mcp-tool-exposure`

**Owner paths.** `codex-rs/config/src/types.rs`；
`core-plugins/src/{loader.rs,manager_tests.rs}`。

**Typed contract.** `PluginMcpServerConfig.omit_tools_from` 使用
`ToolExposureSurface`，loader 仅把它透传到现有 Runtime `McpServerConfig`。筛选、搜索、Tool
identity 和执行仍由上游 MCP/Tool runtime 拥有；本 seam 不引入 selected-root inheritance、
server/tool allowlist 或 Platform policy。

**为什么保留。** 当前 package Role 需要让 server 内可授权 Tool 不出现在初始 `direct` surface，
而保留 Runtime 原生 deferred discovery。

**验证门。** `./scripts/test-codex.sh -p codex-core-plugins`。

**退出条件。** 上游 Plugin MCP policy 原生保留并加载同一 typed omission 字段。

### `managed-child-role-projection-resume`

**Owner paths.** `codex-rs/config/src/config_toml.rs` 与生成的
`core/config.schema.json`；`core/src/{agent/{role.rs,role_tests.rs,control/spawn.rs,control_tests.rs},thread_manager.rs}`；
`app-server/tests/suite/v2/mcp_resource.rs`。

**Typed contract.** 只有 trusted `SessionFlags`
`agents.<role>.runtime_mcp_projection=true` 能在 upstream bounded Role overrides 之外投影
package-owned `instructions`、完整 enabled `skills` 和 typed `mcp_servers`。三个
`include_{permissions,apps,collaboration_mode}_instructions` 只允许显式 `false`；
`model_instructions_file`、`include_environment_context` 和 raw TOML 其余字段不投影。上游
typed model/reasoning/verbosity/personality/service-tier overrides 保留；false-only feature 下降权限
保留，含 `multi_agent_v2=false`。Profile、Workspace、caller source 与 Role 文件本身都不能授予
这个 trust flag。

本地 reapply 只适用于 canonical stored source 为带 Role 的 `SubAgent::ThreadSpawn`、
`InitialHistory::Resumed` 且 Runtime-resolved V1（包括上游未写版本时的 V1 表示）的 cold
resume；它在 Role reload 后恢复原 Thread 的
model/provider/reasoning/verbosity/service tier/cwd/approval/permission snapshot。V2 不走这条本地
路径：upstream `AgentControl` 恢复 Role 并保留 permission snapshot，且只应用一次。当前仓网
Role 在当前 Runtime contract 下 resolve 为 V1；direct app-server V2 child resource-read 不是本地承诺。

**为什么保留。** 当前仓网 cold resume 必须恢复 Data/Network 的 native Role instructions、skills
与 MCP inventory，不能重放 root provider/cwd/approval，也不能信任 Browser/caller 元数据。

**验证门。** `./scripts/test-codex.sh -p codex-config`、
`-p codex-core role`、`-p codex-core ensure_v2_agent_loaded_reloads_registered_unloaded_agent`、
`-p codex-app-server mcp_resource`，以及 config schema drift gate。

**退出条件。** 上游以同一 trust boundary cold-resume V1 child Role 的 package projection，并保留
V2 native AgentControl 行为和 runtime setting snapshot。

### `stdio-mcp-liveness`

**Owner paths.** `codex-rs/rmcp-client/src/{stdio_server_launcher.rs,rmcp_client.rs,executor_process_transport.rs}`；回归只在
`codex-rs/app-server/tests/suite/v2/mcp_server_status.rs` 与 `rmcp-client` 的 owner unit。

**Typed contract.** `StdioServerProcessHandle` 的共享原子 closed state 是一个已启动 stdio
server 的 Runtime-owned liveness 事实。stdio transport 返回 EOF 时、显式 `terminate` 时和最后一个
handle drop 时都只会把该事实收敛为 closed；`RmcpClient::is_closed` 先消费同一状态，再消费 rmcp
service 状态。executor-backed transport 不再另行终止同一 process，避免并发、主动终止、自然退出和
drop 产生重复终态或悬挂的额外终止任务。app-server status 只读取这份 Runtime state，不 ping、重试、
sleep 或建立 Platform cache。

**为什么保留。** comparison snapshot 的 integrated 与 official `76d98a771e6c` 在这些
stdio liveness owner paths 没有等价改动；现有 rmcp service closed 信号可晚于 transport EOF，使
`mcpServerStatus/list` 在自然退出后短暂地继续报告 Connected。

**验证门。** `./scripts/test-codex.sh -p codex-rmcp-client stdio_`；
`./scripts/test-codex.sh -p codex-app-server mcp_server_status_list_reports_disconnected_stdio_transport`。

**退出条件。** upstream 在 stdio process/transport lifecycle 提供等价的共享 closed state，并由
`RmcpClient::is_closed` 消费且保证 executor termination 只有一个 owner 时，删除本地实现和本 seam。

## Followers、同步和分类规则

Protocol/config schema、TypeScript、generated proto、`Cargo.lock`、fixture 与 focused tests 只随
上述 owner 变化。生成物必须通过生成器更新，不能手改，也不能单独作为 seam replay。

同步时先接受上游结构，再按表中顺序重新定位最小 owner；每个非生成差异只能归入一个 seam，或标为
`upstreamed`、`move-out`、`drop`。如果上游已提供等价能力，删除本地实现而不是保留兼容分支。
同步完成后更新 `.sync/codex-upstream.json`、本文件和 machine-readable inventory；库存状态必须
准确区分 deterministic Runtime/app-server/schema 验证与尚未完成的真实 Provider 产品 E2E，不能把
任一层的通过外推成另一层已经通过。

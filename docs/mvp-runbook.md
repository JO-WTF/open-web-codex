# Web 平台本地运行手册

当前本地链路与生产边界一致：

```text
Browser -> open-web-codex-server -> PostgreSQL
                              \-> Profile Host -> codex app-server
                              \-> Run Orchestrator -> Git workspace
```

浏览器只访问同源 REST 和认证 WebSocket。仓库中没有本地 sidecar、无认证
Gateway、原始 JSON-RPC 路由或桌面应用。

## 前置条件

- Node.js 20+、npm、稳定 Rust、Git。
- PostgreSQL 已运行并存在 `open_web_codex` 数据库。
- 真实模式需要当前仓库构建的 Codex，或通过 `CODEX_BIN` 指定兼容 Binary。

默认数据库连接为：

```text
postgresql://$USER@127.0.0.1:5432/open_web_codex
```

## 启动

用 Fake Runtime 验证平台与浏览器：

```bash
./scripts/run-local.sh --fake
```

用真实 Codex 启动：

```bash
./scripts/run-local.sh
```

脚本会构建浏览器、平台服务和缺失的仓库 Codex Binary，执行数据库迁移，
创建私有 Profile/Runner 目录，并在 `http://127.0.0.1:4800/` 提供整个应用。
本地 Secret Store 主密钥首次运行时生成在
`.local/open-web-codex/master-key`，权限为仅当前用户可读；生产部署必须从外部
Secret Manager 注入 `OPEN_WEB_CODEX_MASTER_KEY`。

已有兼容 Binary 时可以显式指定：

```bash
CODEX_BIN=/absolute/path/to/codex ./scripts/run-local.sh
```

含密码的数据库 URL 推荐放在仅当前用户可读的文件中：

```bash
printf '%s\n' 'postgresql://user:password@host:5432/open_web_codex' > .local/database-url
chmod 600 .local/database-url
./scripts/run-local.sh --database-url-file .local/database-url
```

后台管理：

```bash
./scripts/run-local.sh --background
./scripts/run-local.sh --status
./scripts/run-local.sh --stop
```

确认已有构建输出为最新时可使用 `--no-build`。日志位于
`.local/open-web-codex/logs/server.log`。

## 浏览器纵向流程

1. 打开 `http://127.0.0.1:4800/`。
2. 首次运行选择初始化，创建首位 Owner；以后使用登录入口。
3. 创建 Git Project，平台只接受受控 Git URL，不接受浏览器本地路径。
4. 创建 Task 和 Run。Runner 创建私有 mirror 与该 Run 独占的可写 workspace。
5. 向运行中的 Task 发送消息，事件先持久化再通过 WebSocket 投影。
6. 处理待审批请求；浏览器不会看到 app-server request ID 或服务器路径。
7. 在 Changes 中选择文件并显式 Commit。

## 配置

| 变量 | 作用 |
| --- | --- |
| `DATABASE_URL` | PostgreSQL 连接 |
| `DATABASE_MAX_CONNECTIONS` | 连接池大小，默认 10 |
| `CODEX_MODE` | `real` 或 `fake` |
| `CODEX_BIN` | Codex Binary |
| `CODEX_HOME` | 当前 Profile 的持久目录 |
| `OPEN_WEB_CODEX_MASTER_KEY` | Base64 32-byte Secret Store key |
| `OPEN_WEB_CODEX_RUNNER_ROOT` | 私有 mirror/workspace 根目录 |
| `OPEN_WEB_CODEX_DATA_DIR` | 本地状态、PID 和日志目录 |
| `OPEN_WEB_CODEX_BIND_HOST` | 监听地址，默认 `127.0.0.1` |
| `OPEN_WEB_CODEX_SERVER_PORT` | HTTP/WebSocket 端口，默认 `4800` |

## 验证与排错

```bash
curl --fail http://127.0.0.1:4800/api/health
./scripts/run-local.sh --status
```

启动失败先检查 PostgreSQL、`server.log`、Codex Binary 和 Profile Home 权限。
真实模式若配置 Provider Secret，还必须提供稳定的外部主密钥；更换主密钥版本
前需要完成 Secret 轮换，不能直接覆盖旧 key。

## 当前边界

- 当前 Server 组合入口一次启动一个 Profile Host；多用户 Beta 仍需按授权用户
  动态路由多个持久 Profile。
- 已支持隔离 workspace、状态、选择性 Commit；Push 与高级 Diff 尚未作为浏览器
  资源开放。
- Session 当前使用 Bearer token；生产发布前仍需完成 HttpOnly Cookie、CSRF、
  限速、备份恢复和 Runner 强隔离门禁。

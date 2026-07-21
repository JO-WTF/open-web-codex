# Web MVP 本地运行手册

当前 MVP 是一条真实的本地纵向链路：

```text
Browser -> Vite WebApp -> Platform Server
        -> loopback Gateway -> codex app-server -> local workspace
```

它用于尽快验证浏览器运行 Codex 的核心价值，不代表最终的多用户生产架构。Gateway 以无认证模式运行，但强制只绑定 `127.0.0.1`，不得通过反向代理或端口映射对外暴露。

## 启动

```bash
make mvp
```

`make mvp` 在前台运行统一的 `scripts/run-local.sh`，Ctrl-C 会停止由它启动的
全部进程。脚本会：

1. 增量构建仓库的 `codex-cli`，确保运行的 Codex 与当前源码一致。
2. 仅在显式设置 `OPEN_WEB_CODEX_BIN` 时改用外部 Codex Binary。
3. 构建不含语音依赖的 `codex_monitor_daemon`。
4. 构建并启动 Platform Server。
5. 启动 `127.0.0.1:4733` Gateway 和 `127.0.0.1:1420/web` WebApp。
6. 将本地数据和日志放在 `.cache/mvp`。

使用已有 Codex Binary 可缩短首次启动：

```bash
OPEN_WEB_CODEX_BIN=/absolute/path/to/codex make mvp
```

也可以使用统一脚本管理后台实例：

```bash
./scripts/run-local.sh --background
./scripts/run-local.sh --status
./scripts/run-local.sh --restart
./scripts/run-local.sh --stop
```

只有在确认现有 Rust Binary 已是最新版本时，才应跳过增量构建：

```bash
./scripts/run-local.sh --no-build
```

## 浏览器流程

1. 打开 `http://127.0.0.1:1420/web`，状态应为 `online`。
2. 点击 **Load workspaces**；也可以输入服务器本机的绝对路径并点击 **Add**。
3. 选择 Workspace，点击 **Connect**。
4. 点击 **New thread**。
5. 在底部输入任务并点击 **Send**。
6. 中间活动区会显示用户消息和实时 app-server 事件。

## 停止与排错

在前台启动终端按 Ctrl-C，或执行 `./scripts/run-local.sh --stop`，会按 PID
停止该数据目录对应的 Web、Platform Server、Gateway 和 app-server 进程。

日志：

```text
.cache/mvp/logs/daemon.log
.cache/mvp/logs/server.log
.cache/mvp/logs/web.log
.cache/mvp/logs/run-local.log
```

端口可通过以下变量修改，但监听地址始终是 loopback：

```bash
OPEN_WEB_CODEX_WEB_PORT=1420
OPEN_WEB_CODEX_GATEWAY_PORT=4733
OPEN_WEB_CODEX_RPC_PORT=4732
OPEN_WEB_CODEX_SERVER_PORT=4800
```

## MVP 已知限制

- 单用户、单机、可信本地使用。
- 使用 SSE 和 Preview RPC，没有持久事件游标。
- Workspace 输入是服务器本机路径，生产版将改为受控 Git Project。
- 审批 UI、Diff/Commit、身份、RBAC、PostgreSQL 和 Runner 隔离尚未接入。
- Token 仅支持当前浏览器 Session；本地默认无认证模式不需要 Token。

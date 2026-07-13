# ADR-003: 平台 Cargo Workspace 结构

**状态**: 已接受  
**日期**: 2026-07-12  
**关联**: ADR-001, ADR-002, ADR-004

## 背景

当前 `apps/web/` 下的 Rust 代码是 Tauri 应用的 `src-tauri/` crate。平台服务器需要独立于 Tauri 构建, 同时保持与 `apps/web/` 代码库的物理接近。

## 决策

在 `apps/web/` 下创建新的 Cargo workspace, 初始包含以下 crate:

```text
apps/web/
  Cargo.toml              # workspace root
  server/                  # HTTP/S Web 服务器入口
    Cargo.toml
    src/
      main.rs              # CLI 入口
      config.rs            # 配置加载
      routes/
        mod.rs
        health.rs          # Health endpoint
  crates/
    platform-contracts/    # API DTO / 事件类型 / 平台错误
      Cargo.toml
      src/
        lib.rs
        error.rs           # 平台错误类型
        event.rs           # 事件 envelope
        idempotency.rs     # 幂等键类型
    platform-store/        # PostgreSQL 仓库与 migration runner
      Cargo.toml
      src/
        lib.rs
        migrate.rs         # Migration runner
        session.rs         # Session repository (stub)
  migrations/              # SQL migration 文件
    20260712000001_initial.sql
```

## 理由

- 与 Tauri 分离: 独立 workspace 避免与 Tauri 构建和依赖冲突
- 与 Web 代码共址: 保持 React 代码和 Rust 服务器在同一仓库空间
- crate 拆分: M1 目标结构的合理起点, crate 边界与业务领域对齐
- 增量可构建: 每个 crate 独立编译, 层间通过 workspace 依赖

## 关键规则

1. `server` 是唯一二进制 crate; 所有逻辑分布在库 crate 中
2. `platform-contracts` 没有任何运行时依赖 (pure DTO)
3. `platform-store` 依赖 `platform-contracts` 和 sqlx
4. `server` 依赖所有 crate 并提供 Axum/Tower 编排
5. Tauri crate (`src-tauri`) 保留原有结构, 未来逐步迁移

## 影响

- 正向: 清晰的分层; 独立构建; 与 Tauri 无冲突
- 负向: 文件数量增加; 初期一些空 crate 仅做桩

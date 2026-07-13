# ADR-002: 数据库与 Migration

**状态**: 已接受  
**日期**: 2026-07-12  
**关联**: ADR-001, ADR-003

## 背景

平台需要持久化用户、Session、项目、Task、Run、审批和审计数据。数据库选择需要考虑开发体验、迁移管理和与异步运行时的集成。

## 决策

使用 **SQLx 0.9** 配合 **PostgreSQL 16**, 使用 `sqlx migrate` 管理 Schema 变更。

## 理由

- SQLx 提供编译时查询验证 (`sqlx::query!`) 
- 内置 migration runner, 使用纯 SQL 文件, 无需 ORM 学习曲线
- 原生异步支持, 与 Tokio/Axum 无缝集成
- 已在 Codex 工作区使用 (sqlx 0.9)
- PostgreSQL 成熟稳定, 支持 JSON/B 列, 适合事件投影存储

## 关键约定

1. Migration SQL 文件存放在 `apps/web/migrations/` 目录
2. 使用 `sqlx migrate run` 管理 forward migration
3. 所有查询优先使用 `query_as!` 编译时检查, 复杂查询使用 `query_file!`
4. 每个 migration 必须提供回滚 SQL (仅开发环境可用)
5. 表采用 `snake_case` 命名, 列采用 `snake_case`, 所有表包含 `id` (UUID v7) 和 `created_at`/`updated_at`

## 替代方案

- **Diesel**: 编译时检查更好, 但异步支持需要额外处理, 学习曲线更陡
- **SeaORM**: 功能更全但增加了 ORM 抽象层
- **MongoDB**: Schema-less 初期快但后期一致性成本高

## 影响

- 正向: 编译时 SQL 验证; 纯 SQL 文件清晰可评审; 无魔法 ORM 行为
- 负向: 需要 install `sqlx-cli`; 编译时检查需要 DATABASE_URL 环境变量

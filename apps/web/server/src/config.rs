use serde::Deserialize;

/// Platform server configuration loaded from environment / CLI args.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database_url: String,
    pub database_max_connections: u32,
    pub codex_bin: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:4800".to_string(),
            database_url: "postgres://localhost:5432/open_web_codex".to_string(),
            database_max_connections: 10,
            codex_bin: None,
        }
    }
}

/// Returns the current build version string.
pub fn build_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

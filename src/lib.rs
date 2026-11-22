pub mod admin;
pub mod auth;
pub mod catalog;
pub mod config;
pub mod conversations;
pub mod db;
pub mod error;
pub mod files;
pub mod mail;
pub mod membership;
pub mod pages;
pub mod practice;
pub mod security;
use axum::{Router, extract::DefaultBodyLimit, middleware, routing::get};
use deadpool_postgres::Pool;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub config: Arc<config::Config>,
    pub content: Arc<catalog::Content>,
    pub hashes: Arc<Semaphore>,
    pub dummy_hash: Arc<String>,
    pub limits: Arc<std::sync::Mutex<HashMap<String, (u32, std::time::Instant)>>>,
}
impl AppState {
    pub async fn new(config: config::Config) -> anyhow::Result<Self> {
        let pool = db::pool(&config.database).await?;
        let dummy_hash = auth::hash_password("dummy-not-an-account-password".into())
            .await
            .map_err(|e| anyhow::anyhow!(e.1))?;
        Ok(Self {
            pool,
            config: Arc::new(config),
            content: Arc::new(catalog::Content::load()?),
            hashes: Arc::new(Semaphore::new(4)),
            dummy_hash: Arc::new(dummy_hash),
            limits: Arc::default(),
        })
    }
}
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(auth::routes())
        .merge(catalog::routes())
        .merge(membership::routes())
        .merge(admin::routes())
        .merge(practice::routes())
        .merge(conversations::routes())
        .merge(files::routes())
        .merge(pages::routes())
        .layer(DefaultBodyLimit::max(150_000))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::guard,
        ))
        .layer(tower_http::compression::CompressionLayer::new().gzip(true))
        .with_state(state)
}
async fn health(
    axum::extract::State(s): axum::extract::State<AppState>,
) -> error::Result<axum::Json<Value>> {
    s.pool.get().await?.query_one("SELECT 1", &[]).await?;
    Ok(axum::Json(
        serde_json::json!({"ok":true,"service":"eplab-rust"}),
    ))
}

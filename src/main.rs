use eplab_rust::{AppState, config::Config};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "eplab_rust=info".into()),
        )
        .init();
    let state = AppState::new(Config::from_env()?).await?;
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    match command.as_str() {
        "migrate" => eplab_rust::db::migrate(&state.pool).await?,
        "seed" => {
            eplab_rust::db::migrate(&state.pool).await?;
            eplab_rust::catalog::seed(&state).await?;
        }
        "admin" => {
            eplab_rust::db::migrate(&state.pool).await?;
            eplab_rust::auth::create_admin(&state).await?;
        }
        "serve" => {
            eplab_rust::db::migrate(&state.pool).await?;
            eplab_rust::catalog::seed(&state).await?;
            tokio::fs::create_dir_all(state.config.data.join("uploads")).await?;
            let listener = tokio::net::TcpListener::bind(state.config.bind).await?;
            tracing::info!(address=%listener.local_addr()?,"EP Lab Rust ready");
            axum::serve(
                listener,
                eplab_rust::app(state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        }
        _ => anyhow::bail!("Use: eplab-rust [serve|migrate|seed|admin]"),
    }
    Ok(())
}

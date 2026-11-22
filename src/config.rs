use anyhow::{Context, bail};
use std::{env, net::SocketAddr, path::PathBuf};
#[derive(Clone)]
pub struct Config {
    pub database: String,
    pub origin: String,
    pub bind: SocketAddr,
    pub data: PathBuf,
    pub secure: bool,
    pub mail_mode: String,
}
impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let database = env::var("DATABASE_URL")
            .context("Set DATABASE_URL for a dedicated eplab_rust database")?;
        let origin = env::var("APP_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3200".into())
            .trim_end_matches('/')
            .to_string();
        let url = url::Url::parse(&origin)?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("APP_URL must be an origin without credentials, path, query or fragment");
        }
        let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
        if url.scheme() != "https" && !(local && url.scheme() == "http") {
            bail!("Public APP_URL requires HTTPS");
        }
        let mail_mode = env::var("MAIL_MODE").unwrap_or_else(|_| "local".into());
        if !["local", "smtp"].contains(&mail_mode.as_str()) {
            bail!("MAIL_MODE must be local or smtp");
        }
        if !local && mail_mode != "smtp" {
            bail!("Public hosting requires SMTP");
        }
        Ok(Self {
            database,
            secure: url.scheme() == "https",
            origin,
            bind: env::var("BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:3200".into())
                .parse()?,
            data: env::var_os("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("data")),
            mail_mode,
        })
    }
}

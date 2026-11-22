use crate::error::Result;
use deadpool_postgres::GenericClient;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use serde_json::Value;
use tokio_postgres::{NoTls, types::ToSql};

pub async fn pool(url: &str) -> anyhow::Result<Pool> {
    let mut config: tokio_postgres::Config = url.parse()?;
    config.options("-c statement_timeout=15000 -c lock_timeout=5000 -c idle_in_transaction_session_timeout=30000");
    let manager = Manager::from_config(
        config,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    Ok(Pool::builder(manager)
        .max_size(8)
        .runtime(Runtime::Tokio1)
        .wait_timeout(Some(std::time::Duration::from_secs(5)))
        .build()?)
}
pub async fn values<C: GenericClient + Sync>(
    c: &C,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Vec<Value>> {
    Ok(rows(c, sql, params)
        .await?
        .iter()
        .map(|r| r.get::<_, Value>(0))
        .collect())
}
/// Cache only prepared statements on the connection; never cache user data or permissions.
pub async fn rows<C: GenericClient + Sync>(
    c: &C,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Vec<tokio_postgres::Row>> {
    let statement = c.prepare_cached(sql).await?;
    Ok(c.query(&statement, params).await?)
}
pub async fn row<C: GenericClient + Sync>(
    c: &C,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Option<tokio_postgres::Row>> {
    let statement = c.prepare_cached(sql).await?;
    Ok(c.query_opt(&statement, params).await?)
}
pub async fn value<C: GenericClient + Sync>(
    c: &C,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Value> {
    Ok(values(c, sql, params)
        .await?
        .into_iter()
        .next()
        .unwrap_or(Value::Null))
}
pub async fn audit<C: GenericClient + Sync>(
    c: &C,
    actor: Option<uuid::Uuid>,
    action: &str,
    target: &str,
    metadata: Value,
) -> Result<()> {
    c.execute(
        "INSERT INTO audit_logs(actor_id,action,target,metadata) VALUES($1,$2,$3,$4)",
        &[&actor, &action, &target, &metadata],
    )
    .await?;
    Ok(())
}
pub async fn migrate(pool: &Pool) -> anyhow::Result<()> {
    let mut client = pool.get().await?;
    let tx = client.transaction().await?;
    tx.batch_execute("SELECT pg_advisory_xact_lock(8493100)")
        .await?;
    tx.batch_execute("CREATE TABLE IF NOT EXISTS schema_migrations(version integer PRIMARY KEY,checksum text NOT NULL,applied_at timestamptz NOT NULL DEFAULT now())").await?;
    let source = include_str!("../migrations/001_initial.sql");
    let checksum = crate::security::hash(source);
    if let Some(row) = tx
        .query_opt(
            "SELECT checksum FROM schema_migrations WHERE version=1",
            &[],
        )
        .await?
    {
        anyhow::ensure!(
            row.get::<_, String>(0) == checksum,
            "Migration checksum changed; add a new migration instead of changing an applied one"
        );
    } else {
        tx.batch_execute(source).await?;
        tx.execute(
            "INSERT INTO schema_migrations(version,checksum) VALUES(1,$1)",
            &[&checksum],
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

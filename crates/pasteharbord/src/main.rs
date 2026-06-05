mod db;
mod privacy;
mod service;

use anyhow::Context;
use db::Database;
use pasteharbor_core::{BUS_NAME, OBJECT_PATH};
use service::ClipboardService;
use std::path::PathBuf;
use tokio::signal;
use tracing::info;
use zbus::connection::Builder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pasteharbord=info".into()),
        )
        .init();

    let db_path = default_database_path().context("failed to resolve database path")?;
    let database = Database::connect(db_path).await?;
    let service = ClipboardService::new(database);

    let _connection = Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;

    info!("pasteharbord is listening on D-Bus name {BUS_NAME}");
    signal::ctrl_c().await?;
    info!("pasteharbord shutting down");
    Ok(())
}

fn default_database_path() -> anyhow::Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .unwrap_or_else(|| PathBuf::from("."));

    let dir = base.join("pasteharbor");
    std::fs::create_dir_all(&dir)?;

    Ok(dir.join("pasteharbor.db"))
}

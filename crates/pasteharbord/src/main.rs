mod db;
mod privacy;
mod service;

use anyhow::Context;
use db::Database;
use pasteharbor_core::{BUS_NAME, OBJECT_PATH};
use service::ClipboardService;
use std::path::PathBuf;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info};
use zbus::connection::Builder;

const NAME_CHECK_INTERVAL: Duration = Duration::from_secs(5);

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

    // Watchdog: if we ever lose our well-known name (typically because the
    // session bus is torn down and rebuilt on logout/login), the process can
    // stay alive but unreachable, and clients report "pasteharbord is not
    // running". Detect that and exit non-zero so systemd restarts us; the fresh
    // process reacquires the name on the current bus.
    let dbus_proxy = zbus::fdo::DBusProxy::new(&_connection).await?;

    loop {
        tokio::select! {
            result = signal::ctrl_c() => {
                result?;
                info!("pasteharbord shutting down");
                return Ok(());
            }
            _ = tokio::time::sleep(NAME_CHECK_INTERVAL) => {
                let name = zbus::names::BusName::try_from(BUS_NAME)?;
                match dbus_proxy.name_has_owner(name).await {
                    Ok(true) => {}
                    Ok(false) => {
                        error!("lost D-Bus name {BUS_NAME}; exiting so systemd can restart");
                        std::process::exit(1);
                    }
                    Err(error) => {
                        error!("D-Bus connection check failed ({error}); exiting so systemd can restart");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
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

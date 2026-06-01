use crate::db::Database;
use crate::privacy::should_skip_text;
use serde_json::json;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use zbus::fdo;
use zbus::interface;

#[derive(Clone)]
pub struct ClipboardService {
    database: Database,
}

impl ClipboardService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[interface(name = "io.github.pasteharbor.Clipboard1")]
impl ClipboardService {
    async fn capture_text(
        &self,
        text: &str,
        source_app: &str,
        sensitive: bool,
    ) -> fdo::Result<i64> {
        if should_skip_text(text, sensitive) {
            return Ok(0);
        }

        self.database
            .capture_text(
                text,
                Some(source_app).filter(|value| !value.is_empty()),
                sensitive,
            )
            .await
            .map_err(failed)
    }

    async fn list_recent(&self, limit: u32) -> fdo::Result<String> {
        let items = self.database.list_recent(limit).await.map_err(failed)?;
        serde_json::to_string(&items).map_err(failed)
    }

    async fn search(&self, query: &str, limit: u32) -> fdo::Result<String> {
        let items = self.database.search(query, limit).await.map_err(failed)?;
        serde_json::to_string(&items).map_err(failed)
    }

    async fn get_text(&self, id: i64) -> fdo::Result<String> {
        let text = self.database.get_text(id).await.map_err(failed)?;

        match text {
            Some(text) => {
                self.database.touch(id).await.map_err(failed)?;
                Ok(text)
            }
            None => Err(fdo::Error::FileNotFound(format!(
                "clipboard item {id} not found"
            ))),
        }
    }

    async fn delete_item(&self, id: i64) -> fdo::Result<bool> {
        self.database.delete_item(id).await.map_err(failed)
    }

    async fn clear(&self) -> fdo::Result<u64> {
        self.database.clear().await.map_err(failed)
    }

    async fn get_settings(&self) -> fdo::Result<String> {
        let settings = self.database.get_settings().await.map_err(failed)?;
        serde_json::to_string(&settings).map_err(failed)
    }

    async fn set_max_history(&self, max_history: u32) -> fdo::Result<u32> {
        self.database
            .set_max_history(max_history)
            .await
            .map_err(failed)
    }

    async fn show_app(&self) -> fdo::Result<bool> {
        launch_app().map_err(failed)?;
        Ok(true)
    }

    async fn health(&self) -> fdo::Result<String> {
        Ok(json!({"ok": true, "service": "pasteharbord"}).to_string())
    }
}

fn launch_app() -> anyhow::Result<()> {
    let mut child = Command::new(app_executable()).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn app_executable() -> PathBuf {
    if let Some(path) = env::var_os("PASTEHARBOR_APP") {
        return PathBuf::from(path);
    }

    if let Ok(current_exe) = env::current_exe() {
        let sibling = current_exe.with_file_name("pasteharbor-app");
        if sibling.exists() {
            return sibling;
        }
    }

    PathBuf::from("pasteharbor-app")
}

fn failed(error: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(error.to_string())
}

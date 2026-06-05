use anyhow::Context;
use chrono::Utc;
use pasteharbor_core::{
    normalize_max_history, AppSettings, ClipboardItemSummary, DEFAULT_MAX_HISTORY,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;

const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_PREVIEW_CHARS: usize = 180;
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn connect(path: PathBuf) -> anyhow::Result<Self> {
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .with_context(|| format!("failed to open SQLite database at {}", path.display()))?;

        let database = Self { pool };
        database.migrate().await?;
        Ok(database)
    }

    pub async fn capture_text(
        &self,
        text: &str,
        source_app: Option<&str>,
        sensitive: bool,
    ) -> anyhow::Result<i64> {
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Ok(0);
        }

        let now = Utc::now().to_rfc3339();
        let hash = blake3::hash(text.as_bytes()).to_hex().to_string();

        let id = if let Some(existing_id) = self.find_by_hash(&hash).await? {
            sqlx::query("UPDATE clipboard_items SET last_used_at = ? WHERE id = ?")
                .bind(&now)
                .bind(existing_id)
                .execute(&self.pool)
                .await?;
            existing_id
        } else {
            let result = sqlx::query(
                r#"
                INSERT INTO clipboard_items (
                    created_at,
                    last_used_at,
                    kind,
                    mime_types,
                    preview_text,
                    content_text,
                    content_hash,
                    source_app,
                    sensitive,
                    size_bytes
                )
                VALUES (?, ?, 'text', ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&now)
            .bind(&now)
            .bind(r#"["text/plain;charset=utf-8","text/plain"]"#)
            .bind(preview_text(text))
            .bind(text)
            .bind(hash)
            .bind(source_app)
            .bind(i64::from(sensitive))
            .bind(text.len() as i64)
            .execute(&self.pool)
            .await?;

            result.last_insert_rowid()
        };

        self.prune_to_max_history().await?;
        Ok(id)
    }

    pub async fn list_recent(&self, limit: u32) -> anyhow::Result<Vec<ClipboardItemSummary>> {
        self.query_items(None, limit).await
    }

    pub async fn search(
        &self,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<ClipboardItemSummary>> {
        self.query_items(Some(query), limit).await
    }

    pub async fn get_text(&self, id: i64) -> anyhow::Result<Option<String>> {
        let row =
            sqlx::query("SELECT content_text FROM clipboard_items WHERE id = ? AND kind = 'text'")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.and_then(|row| {
            row.try_get::<Option<String>, _>("content_text")
                .ok()
                .flatten()
        }))
    }

    pub async fn touch(&self, id: i64) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE clipboard_items SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_item(&self, id: i64) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM clipboard_items WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn clear(&self) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM clipboard_items")
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn get_settings(&self) -> anyhow::Result<AppSettings> {
        Ok(AppSettings {
            max_history: self.get_max_history().await?,
        })
    }

    pub async fn set_max_history(&self, max_history: u32) -> anyhow::Result<u32> {
        let max_history = normalize_max_history(max_history);
        sqlx::query(
            r#"
            INSERT INTO app_settings (key, value)
            VALUES ('max_history', ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
        )
        .bind(max_history.to_string())
        .execute(&self.pool)
        .await?;

        self.prune_history(max_history).await?;
        Ok(max_history)
    }

    async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS clipboard_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                last_used_at TEXT NOT NULL,
                kind TEXT NOT NULL,
                mime_types TEXT NOT NULL,
                preview_text TEXT NOT NULL,
                content_text TEXT,
                content_hash TEXT NOT NULL UNIQUE,
                source_app TEXT,
                sensitive INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_items_last_used_at ON clipboard_items(last_used_at DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_items_preview_text ON clipboard_items(preview_text)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO app_settings (key, value)
            VALUES ('max_history', ?)
            "#,
        )
        .bind(DEFAULT_MAX_HISTORY.to_string())
        .execute(&self.pool)
        .await?;

        self.prune_to_max_history().await?;
        Ok(())
    }

    async fn find_by_hash(&self, hash: &str) -> anyhow::Result<Option<i64>> {
        let row = sqlx::query("SELECT id FROM clipboard_items WHERE content_hash = ?")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|row| row.get::<i64, _>("id")))
    }

    async fn get_max_history(&self) -> anyhow::Result<u32> {
        let value = sqlx::query("SELECT value FROM app_settings WHERE key = 'max_history'")
            .fetch_optional(&self.pool)
            .await?
            .and_then(|row| row.try_get::<String, _>("value").ok())
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_HISTORY);

        Ok(normalize_max_history(value))
    }

    async fn prune_to_max_history(&self) -> anyhow::Result<()> {
        let max_history = self.get_max_history().await?;
        self.prune_history(max_history).await
    }

    async fn prune_history(&self, max_history: u32) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            DELETE FROM clipboard_items
            WHERE id NOT IN (
                SELECT id
                FROM clipboard_items
                ORDER BY last_used_at DESC, id DESC
                LIMIT ?
            )
            "#,
        )
        .bind(i64::from(max_history))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn query_items(
        &self,
        query: Option<&str>,
        limit: u32,
    ) -> anyhow::Result<Vec<ClipboardItemSummary>> {
        let capped_limit = limit.clamp(1, 250) as i64;

        let rows = if let Some(query) = query.filter(|query| !query.trim().is_empty()) {
            let pattern = format!("%{}%", query.trim());
            sqlx::query(
                r#"
                SELECT id, created_at, last_used_at, kind, preview_text, source_app, size_bytes
                FROM clipboard_items
                WHERE preview_text LIKE ?
                ORDER BY last_used_at DESC, id DESC
                LIMIT ?
                "#,
            )
            .bind(pattern)
            .bind(capped_limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, created_at, last_used_at, kind, preview_text, source_app, size_bytes
                FROM clipboard_items
                ORDER BY last_used_at DESC, id DESC
                LIMIT ?
                "#,
            )
            .bind(capped_limit)
            .fetch_all(&self.pool)
            .await?
        };

        rows.into_iter()
            .map(|row| {
                Ok(ClipboardItemSummary {
                    id: row.try_get("id")?,
                    created_at: row.try_get("created_at")?,
                    last_used_at: row.try_get("last_used_at")?,
                    kind: row.try_get("kind")?,
                    preview_text: row.try_get("preview_text")?,
                    source_app: row.try_get("source_app")?,
                    size_bytes: row.try_get("size_bytes")?,
                })
            })
            .collect()
    }
}

fn preview_text(text: &str) -> String {
    let mut preview = text.split_whitespace().collect::<Vec<_>>().join(" ");

    if preview.chars().count() > MAX_PREVIEW_CHARS {
        preview = preview.chars().take(MAX_PREVIEW_CHARS).collect();
        preview.push_str("...");
    }

    preview
}

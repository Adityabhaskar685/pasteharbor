use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.pasteharbor.App";
pub const BUS_NAME: &str = "io.github.pasteharbor";
pub const OBJECT_PATH: &str = "/io/github/pasteharbor/Clipboard1";
pub const INTERFACE: &str = "io.github.pasteharbor.Clipboard1";

pub const DEFAULT_MAX_HISTORY: u32 = 500;
pub const MIN_MAX_HISTORY: u32 = 10;
pub const MAX_MAX_HISTORY: u32 = 10_000;

pub mod method {
    pub const CAPTURE_TEXT: &str = "CaptureText";
    pub const LIST_RECENT: &str = "ListRecent";
    pub const SEARCH: &str = "Search";
    pub const GET_TEXT: &str = "GetText";
    pub const DELETE_ITEM: &str = "DeleteItem";
    pub const CLEAR: &str = "Clear";
    pub const GET_SETTINGS: &str = "GetSettings";
    pub const SET_MAX_HISTORY: &str = "SetMaxHistory";
    pub const SHOW_APP: &str = "ShowApp";
    pub const HEALTH: &str = "Health";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardItemSummary {
    pub id: i64,
    pub created_at: String,
    pub last_used_at: String,
    pub kind: String,
    pub preview_text: String,
    pub source_app: Option<String>,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    pub max_history: u32,
}

pub fn normalize_max_history(value: u32) -> u32 {
    value.clamp(MIN_MAX_HISTORY, MAX_MAX_HISTORY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_item_summary_round_trips_json() {
        let item = ClipboardItemSummary {
            id: 42,
            created_at: "2026-06-05T10:00:00Z".to_string(),
            last_used_at: "2026-06-05T10:01:00Z".to_string(),
            kind: "text".to_string(),
            preview_text: "hello from PasteHarbor".to_string(),
            source_app: Some("gnome-shell".to_string()),
            size_bytes: 22,
        };

        let json = serde_json::to_string(&item).unwrap();
        let decoded: ClipboardItemSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, item);
    }

    #[test]
    fn app_settings_round_trips_json() {
        let settings = AppSettings { max_history: 250 };

        let json = serde_json::to_string(&settings).unwrap();
        let decoded: AppSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, settings);
    }

    #[test]
    fn normalizes_max_history_bounds() {
        assert_eq!(normalize_max_history(0), MIN_MAX_HISTORY);
        assert_eq!(
            normalize_max_history(DEFAULT_MAX_HISTORY),
            DEFAULT_MAX_HISTORY
        );
        assert_eq!(normalize_max_history(u32::MAX), MAX_MAX_HISTORY);
    }
}

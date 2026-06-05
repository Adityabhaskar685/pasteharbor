use std::cell::Cell;

pub type ClipboardItem = pasteharbor_core::ClipboardItemSummary;

pub struct UiSettings {
    pub auto_refresh: Cell<bool>,
    pub row_limit: Cell<u32>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            auto_refresh: Cell::new(true),
            row_limit: Cell::new(75),
        }
    }
}

use pasteharbor_core::{method, AppSettings, BUS_NAME, INTERFACE, OBJECT_PATH};
use std::cell::RefCell;
use std::rc::Rc;
use zbus::blocking::{Connection, Proxy};

#[derive(Clone)]
pub struct AppState {
    connection: Rc<RefCell<Option<Connection>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connection: Rc::new(RefCell::new(None)),
        }
    }

    pub fn items_json(&self, query: &str, limit: u32) -> anyhow::Result<String> {
        let proxy = self.proxy()?;
        if query.trim().is_empty() {
            Ok(proxy.call(method::LIST_RECENT, &(limit))?)
        } else {
            Ok(proxy.call(method::SEARCH, &(query, limit))?)
        }
    }

    pub fn get_text(&self, id: i64) -> anyhow::Result<String> {
        let proxy = self.proxy()?;
        Ok(proxy.call(method::GET_TEXT, &(id))?)
    }

    pub fn get_image(&self, id: i64) -> anyhow::Result<(Vec<u8>, String)> {
        let proxy = self.proxy()?;
        Ok(proxy.call(method::GET_IMAGE, &(id))?)
    }

    pub fn get_thumbnail(&self, id: i64) -> anyhow::Result<Vec<u8>> {
        let proxy = self.proxy()?;
        Ok(proxy.call(method::GET_THUMBNAIL, &(id))?)
    }

    pub fn delete_item(&self, id: i64) -> anyhow::Result<bool> {
        let proxy = self.proxy()?;
        Ok(proxy.call(method::DELETE_ITEM, &(id))?)
    }

    pub fn clear(&self) -> anyhow::Result<u64> {
        let proxy = self.proxy()?;
        Ok(proxy.call(method::CLEAR, &())?)
    }

    pub fn settings(&self) -> anyhow::Result<AppSettings> {
        let proxy = self.proxy()?;
        let json: String = proxy.call(method::GET_SETTINGS, &())?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn set_max_history(&self, max_history: u32) -> anyhow::Result<u32> {
        let proxy = self.proxy()?;
        Ok(proxy.call(method::SET_MAX_HISTORY, &(max_history))?)
    }

    fn proxy(&self) -> anyhow::Result<Proxy<'_>> {
        if self.connection.borrow().is_none() {
            *self.connection.borrow_mut() = Some(Connection::session()?);
        }

        let borrow = self.connection.borrow();
        let connection = borrow.as_ref().expect("connection initialized");
        Ok(Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE)?)
    }
}

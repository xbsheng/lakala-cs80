use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default)]
    pub version: u32,
    #[serde(rename = "activePage", default)]
    pub active_page: Option<String>,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub theme: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub widgets: Vec<Widget>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub cell: [i32; 4],
    #[serde(default)]
    pub config: Value,
}

impl Layout {
    pub fn active_page(&self) -> Option<&Page> {
        if let Some(id) = &self.active_page {
            if let Some(p) = self.pages.iter().find(|p| &p.id == id) {
                return Some(p);
            }
        }
        self.pages.first()
    }
}

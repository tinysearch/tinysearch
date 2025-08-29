use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub meta: HashMap<String, String>,
    pub body: Option<String>,
}

pub type Posts = Vec<Post>;

pub fn read(raw: String) -> Result<Posts, serde_json::Error> {
    serde_json::from_str(&raw)
}

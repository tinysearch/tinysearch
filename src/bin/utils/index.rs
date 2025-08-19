use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

pub type Posts = Vec<Post>;

pub fn read(raw: String) -> Result<Posts, serde_json::Error> {
    serde_json::from_str(&raw)
}

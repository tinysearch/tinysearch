use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    pub title: String,
    pub url: String,
    pub meta: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlexiblePost {
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

pub type Posts = Vec<Post>;
pub type FlexiblePosts = Vec<FlexiblePost>;

pub fn read(raw: String) -> Result<Posts, serde_json::Error> {
    serde_json::from_str(&raw)
}

pub fn read_flexible(raw: String) -> Result<FlexiblePosts, serde_json::Error> {
    serde_json::from_str(&raw)
}

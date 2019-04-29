#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    pub title: String,
    pub url: String,
    pub body: String,
}

pub type Posts = Vec<Post>;

pub fn read(raw: String) -> Result<Posts, serde_json::Error> {
    serde_json::from_str(&raw)
}

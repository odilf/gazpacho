use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Video {
    name: String,
    category: String,
    path: PathBuf,
    failed: Option<String>,
    meta: serde_json::Value,
}

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub commit: String,
    pub val_bpb: f64,
    pub memory_gb: f64,
    pub status: String,
    pub description: String,
    pub timestamp: String,
    pub params: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLog {
    pub experiments: Vec<Experiment>,
    pub run_tag: String,
    pub created_at: String,
}

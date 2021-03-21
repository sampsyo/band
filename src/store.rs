use serde::{Serialize, Deserialize};
use chrono::prelude::*;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub body: String,
    pub user: String,
    pub ts: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Session {
    pub user: String,
    pub ts: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Store {
    pub db: sled::Db,
}

impl Store {
    pub fn new<P: AsRef<Path>>(path: P) -> sled::Result<Store> {
        let db = sled::open(path)?;
        Ok(Store { db })
    }
}

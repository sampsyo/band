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

    pub fn message_tree(&self, room_id: &str) -> sled::Result<sled::Tree> {
        // this could surely be made more efficient using byte manipulation instead of format!
        let tree_name = format!("msgs:{}", room_id);
        self.db.open_tree(tree_name)
    }

    pub fn session_tree(&self, room_id: &str) -> sled::Result<sled::Tree> {
        // as above
        let tree_name = format!("sess:{}", room_id);
        self.db.open_tree(tree_name)
    }

    pub fn room_exists(&self, room_id: &str) -> sled::Result<bool> {
        let rooms = self.db.open_tree("rooms")?;
        rooms.contains_key(room_id)
    }

    pub fn create_session(&self, room_id: &str, user: &str) -> tide::Result<u64> {
        let session = Session {
            user: user.to_string(),
            ts: Utc::now(),
        };

        let id = self.db.generate_id()?;
        let sessions = self.session_tree(room_id)?;
        let data = serde_json::to_vec(&session)?;
        sessions.insert(id.to_be_bytes(), data)?;

        Ok(id)
    }
}

use serde::{Serialize, Deserialize};
use chrono::prelude::*;
use std::path::Path;

pub type Id = u64;

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

    pub fn message_tree(&self, room_id: Id) -> sled::Result<sled::Tree> {
        // this could surely be made more efficient using byte manipulation instead of format!
        let tree_name = format!("msgs:{}", room_id);
        self.db.open_tree(tree_name)
    }

    pub fn session_tree(&self, room_id: Id) -> sled::Result<sled::Tree> {
        // as above
        let tree_name = format!("sess:{}", room_id);
        self.db.open_tree(tree_name)
    }

    pub fn room_exists(&self, room_id: Id) -> sled::Result<bool> {
        let rooms = self.db.open_tree("rooms")?;
        rooms.contains_key(room_id.to_be_bytes())
    }

    pub fn create_session(&self, room_id: Id, user: &str) -> tide::Result<Id> {
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

    pub fn add_message(&self, room_id: Id, msg: &Message) -> tide::Result<Id> {
        let msgs = self.message_tree(room_id)?;
        let msg_id = self.db.generate_id()?;

        let data = serde_json::to_vec(&msg)?;
        msgs.insert(msg_id.to_be_bytes(), data)?;

        Ok(msg_id)
    }

    pub fn add_room(&self) -> sled::Result<u64> {
        let rooms = self.db.open_tree("rooms")?;
        let id = self.db.generate_id()?;
        rooms.insert(id.to_be_bytes(), vec![])?;  // Currently just for existence.
        Ok(id)
    }
}

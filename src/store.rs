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

fn scoped_id(scope: u8, id: u64) -> [u8; 9] {
    let mut res = [0; 9];
    res[0] = scope;
    res[1..].clone_from_slice(&id.to_be_bytes());
    res
}

impl Store {
    pub fn new<P: AsRef<Path>>(path: P) -> sled::Result<Store> {
        let db = sled::open(path)?;
        Ok(Store { db })
    }

    fn message_tree(&self, room_id: Id) -> sled::Result<sled::Tree> {
        self.db.open_tree(scoped_id(0, room_id))
    }

    fn session_tree(&self, room_id: Id) -> sled::Result<sled::Tree> {
        self.db.open_tree(scoped_id(1, room_id))
    }

    pub fn room_exists(&self, room_id: Id) -> sled::Result<bool> {
        let rooms = self.db.open_tree("rooms")?;
        rooms.contains_key(room_id.to_be_bytes())
    }

    pub fn add_session(&self, room_id: Id, session: &Session) -> tide::Result<Id> {
        let sessions = self.session_tree(room_id)?;
        let id = self.db.generate_id()?;

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

    pub fn all_messages(&self, room_id: Id) -> sled::Result<Vec<Message>> {
        let msgs = self.message_tree(room_id)?;
        let all_msgs: Result<Vec<_>, _> = msgs.iter().values().map(|r| {
            r.map(|data| serde_json::from_slice::<Message>(&data).unwrap())
        }).collect();
        all_msgs
    }
}

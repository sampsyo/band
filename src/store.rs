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

fn insert_ser<T: Serialize>(tree: &sled::Tree, id: Id, val: &T) -> sled::Result<()> {
    tree.insert(id.to_be_bytes(), bincode::serialize(&val).unwrap())?;
    Ok(())
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

    fn room_tree(&self) -> sled::Result<sled::Tree> {
        self.db.open_tree([3])
    }

    pub fn room_exists(&self, room_id: Id) -> sled::Result<bool> {
        let rooms = self.room_tree()?;
        rooms.contains_key(room_id.to_be_bytes())
    }

    pub fn add_session(&self, room_id: Id, session: &Session) -> sled::Result<Id> {
        let id = self.db.generate_id()?;
        insert_ser(&self.session_tree(room_id)?, id, &session)?;
        Ok(id)
    }

    pub fn add_message(&self, room_id: Id, msg: &Message) -> sled::Result<Id> {
        let id = self.db.generate_id()?;
        insert_ser(&self.message_tree(room_id)?, id, &msg)?;
        Ok(id)
    }

    pub fn add_room(&self) -> sled::Result<u64> {
        let id = self.db.generate_id()?;
        self.room_tree()?.insert(id.to_be_bytes(), vec![])?;  // Currently just for existence.
        Ok(id)
    }

    pub fn all_messages(&self, room_id: Id) -> sled::Result<Vec<Message>> {
        let msgs = self.message_tree(room_id)?;
        let all_msgs: Result<Vec<_>, _> = msgs.iter().values().map(|r| {
            r.map(|data| bincode::deserialize::<Message>(&data).unwrap())
        }).collect();
        all_msgs
    }
}

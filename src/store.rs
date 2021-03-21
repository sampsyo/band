use serde::{Serialize, Deserialize};
use chrono::prelude::*;
use std::path::Path;
use std::convert::TryInto;
use std::collections::HashMap;

pub type Id = u64;

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub body: String,
    pub session: Id,
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

fn values_des<T: serde::de::DeserializeOwned>(tree: &sled::Tree) -> impl Iterator<Item=sled::Result<T>> {
    tree.iter().values().map(|r| {
        r.map(|data| bincode::deserialize(&data).unwrap())
    })
}

fn iter_des<T: serde::de::DeserializeOwned>(tree: &sled::Tree) -> impl Iterator<Item=sled::Result<(Id, T)>> {
    tree.iter().map(|r| {
        r.map(|(k, v)| (u64::from_be_bytes((*k).try_into().unwrap()), bincode::deserialize(&v).unwrap()))
    })
}

impl Store {
    pub fn new<P: AsRef<Path>>(path: P) -> sled::Result<Store> {
        let db = sled::open(path)?;
        Ok(Store { db })
    }

    fn message_tree(&self, room: Id) -> sled::Result<sled::Tree> {
        self.db.open_tree(scoped_id(0, room))
    }

    fn session_tree(&self, room: Id) -> sled::Result<sled::Tree> {
        self.db.open_tree(scoped_id(1, room))
    }

    fn room_tree(&self) -> sled::Result<sled::Tree> {
        self.db.open_tree([3])
    }

    pub fn room_exists(&self, room: Id) -> sled::Result<bool> {
        let rooms = self.room_tree()?;
        rooms.contains_key(room.to_be_bytes())
    }

    pub fn get_session(&self, room: Id, session: Id) -> sled::Result<Option<Session>> {
        let sessions = self.session_tree(room)?;
        let data = sessions.get(session.to_be_bytes())?;
        Ok(data.map(|d| {
            bincode::deserialize(&d).unwrap()
        }))
    }

    pub fn add_session(&self, room: Id, session: &Session) -> sled::Result<Id> {
        let id: u64 = rand::random();  // Unpredictable id.
        insert_ser(&self.session_tree(room)?, id, &session)?;
        Ok(id)
    }

    pub fn add_message(&self, room: Id, msg: &Message) -> sled::Result<Id> {
        let id = self.db.generate_id()?;  // Sequential id.
        insert_ser(&self.message_tree(room)?, id, &msg)?;
        Ok(id)
    }

    pub fn add_room(&self) -> sled::Result<u64> {
        let id: u64 = rand::random();  // Unpredictable id.
        self.room_tree()?.insert(id.to_be_bytes(), vec![])?;  // Currently just for existence.
        Ok(id)
    }

    pub fn iter_messages(&self, room: Id) -> sled::Result<impl Iterator<Item=sled::Result<Message>>> {
        let msgs = self.message_tree(room)?;
        Ok(values_des::<Message>(&msgs))
    }

    pub fn all_sessions(&self, room: Id) -> sled::Result<HashMap<Id, Session>> {
        let sessions = self.session_tree(room)?;
        iter_des::<Session>(&sessions).collect()
    }

    pub fn set_user(&self, room: Id, session: Id, user: &str) -> sled::Result<Option<()>> {
        let sessions = self.session_tree(room)?;
        let res = sessions.fetch_and_update(session.to_be_bytes(), |old| {
            old.map(|data| {
                let sess: Session = bincode::deserialize(data).unwrap();
                let new = Session { user: user.to_string(), ..sess };
                bincode::serialize(&new).unwrap()
            })
        })?;
        Ok(res.map(|_| ()))
    }
}

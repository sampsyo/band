use tera::Tera;
use tide::{Body, log};
use tide_tera::prelude::*;
use broadcaster::BroadcastChannel;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use chrono::prelude::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Message {
    body: String,
    user: String,
    ts: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Session {
    user: String,
    ts: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
struct IncomingMessage {
    body: String,
    user: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct IncomingSession {
    user: String,
}

type Channel = BroadcastChannel<Message>;

#[derive(Clone)]
struct State {
    tera: Tera,
    chans: Arc<Mutex<HashMap<String, Channel>>>,
    db: sled::Db,
    harsh: harsh::Harsh,
}

impl State {
    fn get_chan(&self, room_id: &str) -> Channel {
        let chans = &mut self.chans.lock().unwrap();
        match chans.get(room_id) {
            Some(c) => c.clone(),
            None => {
                let chan = BroadcastChannel::new();
                chans.insert(room_id.to_string(), chan.clone());
                chan
            },
        }
    }

    fn message_tree(&self, room_id: &str) -> sled::Result<sled::Tree> {
        // this could surely be made more efficient using byte manipulation instead of format!
        let tree_name = format!("msgs:{}", room_id);
        self.db.open_tree(tree_name)
    }

    fn session_tree(&self, room_id: &str) -> sled::Result<sled::Tree> {
        // as above
        let tree_name = format!("sess:{}", room_id);
        self.db.open_tree(tree_name)
    }

    fn create_room(&self) -> sled::Result<String> {
        let id = self.db.generate_id()?;
        let id_str = self.harsh.encode(&[id]);  // TODO: Actually use numbers as IDs??

        let rooms = self.db.open_tree("rooms")?;
        rooms.insert(&id_str, vec![])?;  // Currently just for existence.
        Ok(id_str)
    }

    fn room_exists(&self, room_id: &str) -> sled::Result<bool> {
        let rooms = self.db.open_tree("rooms")?;
        rooms.contains_key(room_id)
    }

    fn room_or_404(&self, room_id: &str) -> tide::Result<()> {
        if self.room_exists(room_id)? {
            Ok(())
        } else {
            Err(tide::Error::from_str(404, "unknown room"))
        }
    }

    async fn send_message(&self, room_id: &str, incoming: IncomingMessage) -> tide::Result<()> {
        let msg = Message {
            body: incoming.body,
            user: incoming.user,
            ts: Utc::now(),
        };

        // Send to connected clients.
        let chan = self.get_chan(room_id);
        chan.send(&msg).await?;

        // Record message in the history database.
        let msgs = self.message_tree(room_id)?;
        let msg_id = self.db.generate_id()?.to_be_bytes();

        let data = serde_json::to_vec(&msg)?;
        msgs.insert(msg_id, data)?;

        Ok(())
    }

    fn create_session(&self, room_id: &str, incoming: IncomingSession) -> tide::Result<String> {
        let session = Session {
            user: incoming.user,
            ts: Utc::now(),
        };

        let id = self.db.generate_id()?;
        let sessions = self.session_tree(room_id)?;
        let data = serde_json::to_vec(&session)?;
        sessions.insert(id.to_be_bytes(), data)?;

        Ok(self.harsh.encode(&[id]))
    }
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let room_id = req.param("room")?;
    req.state().room_or_404(room_id)?;
    let mut chan = req.state().get_chan(room_id);

    while let Some(msg) = chan.next().await {
        log::debug!("emitting message: {:?}", msg);
        let data = serde_json::to_string(&msg)?;
        sender.send("message", data, None).await?;
    }

    Ok(())
}

async fn chat_send(mut req: tide::Request<State>) -> tide::Result {
    let msg: IncomingMessage = req.body_json().await?;
    let room_id = req.param("room")?;
    req.state().room_or_404(room_id)?;

    log::debug!("received message in {}: {:?}", room_id, msg);
    req.state().send_message(&room_id, msg).await?;
    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn chat_page(req: tide::Request<State>) -> tide::Result {
    let room_id = req.param("room")?;

    // Make sure we stop with a 404 if the room does not exist.
    req.state().room_or_404(room_id)?;

    let tera = &req.state().tera;
    tera.render_response("chat.html", &context! {
        "room_id" => room_id
    })
}

async fn chat_history(req: tide::Request<State>) -> tide::Result<Body> {
    let room_id = req.param("room")?;

    let msgs = req.state().message_tree(room_id)?;
    let all_msgs: Result<Vec<_>, _> = msgs.iter().values().map(|r| {
        r.map(|data| serde_json::from_slice::<Message>(&data).unwrap())
    }).collect();

    Ok(Body::from_json(&all_msgs?)?)
}

async fn make_chat(req: tide::Request<State>) -> tide::Result {
    let room_id = req.state().create_room()?;
    req.state().get_chan(&room_id);  // Eagerly materialize the channel.
    Ok(tide::Redirect::new(format!("/{}", room_id)).into())
}

async fn make_session(mut req: tide::Request<State>) -> tide::Result {
    let data: IncomingSession = req.body_json().await?;
    let room_id = req.param("room")?;
    req.state().room_or_404(room_id)?;

    let id = req.state().create_session(&room_id, data)?;
    Ok(tide::Response::from(id))
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let db = sled::open("band.db")?;

    log::with_level(log::LevelFilter::Debug);
    let mut app = tide::with_state(State {
        tera,
        chans: Arc::new(Mutex::new(HashMap::new())),
        db,
        harsh: harsh::Harsh::default(),
    });

    app.at("/:room/chat").get(tide::sse::endpoint(chat_stream));
    app.at("/:room").get(chat_page);
    app.at("/:room/send").post(chat_send);
    app.at("/:room/history").get(chat_history);
    app.at("/:room/session").post(make_session);

    app.at("/new").post(make_chat);
    app.at("/").get(|req: tide::Request<State>| async move {
        let tera = &req.state().tera;
        tera.render_response("home.html", &context! {})
    });

    app.at("/static").serve_dir("static/")?;

    app.listen("localhost:8080").await?;
    Ok(())
}

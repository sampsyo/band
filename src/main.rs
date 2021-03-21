use tera::Tera;
use tide::{Body, log};
use tide_tera::prelude::*;
use broadcaster::BroadcastChannel;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use chrono::prelude::*;

mod store;

#[derive(Serialize, Debug, Clone)]
struct OutgoingMessage {
    body: String,
    user: String,
    ts: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
struct IncomingSession {
    user: String,
}

type Channel = BroadcastChannel<OutgoingMessage>;

#[derive(Clone)]
struct State {
    tera: Tera,
    chans: Arc<Mutex<HashMap<store::Id, Channel>>>,
    store: store::Store,
    harsh: harsh::Harsh,
}

impl OutgoingMessage {
    fn new(msg: &store::Message, sess: &store::Session) -> OutgoingMessage {
        OutgoingMessage {
            body: msg.body.clone(),
            user: sess.user.clone(),
            ts: msg.ts,
        }
    }
}

impl State {
    fn get_chan(&self, room_id: store::Id) -> Channel {
        let chans = &mut self.chans.lock().unwrap();
        match chans.get(&room_id) {
            Some(c) => c.clone(),
            None => {
                let chan = BroadcastChannel::new();
                chans.insert(room_id, chan.clone());
                chan
            },
        }
    }

    fn room_or_404(&self, room_id: &str) -> tide::Result<store::Id> {
        let id = self.parse_id(&room_id)?;
        if self.store.room_exists(id)? {
            Ok(id)
        } else {
            Err(tide::Error::from_str(404, "unknown room"))
        }
    }

    fn sess_or_404(&self, room: store::Id, sess_id: &str) -> tide::Result<(store::Id, store::Session)> {
        let id = self.parse_id(&sess_id)?;
        match self.store.get_session(room, id)? {
            Some(s) => Ok((id, s)),
            None => Err(tide::Error::from_str(404, "unknown session")),
        }
    }

    async fn send_message(&self, room_id: store::Id, session_id: store::Id, session: &store::Session, body: String) -> tide::Result<()> {
        // Record message in the history database.
        let msg = store::Message {
            body,
            session: session_id,
            ts: Utc::now(),
        };
        self.store.add_message(room_id, &msg)?;

        // Send to connected clients.
        let outgoing = OutgoingMessage::new(&msg, &session);
        self.get_chan(room_id).send(&outgoing).await?;

        Ok(())
    }

    pub fn fmt_id(&self, id: u64) -> String {
        self.harsh.encode(&[id])
    }

    pub fn parse_id(&self, id: &str) -> Result<u64, harsh::Error> {
        Ok(self.harsh.decode(id)?[0])
    }
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let mut chan = req.state().get_chan(room_id);

    while let Some(msg) = chan.next().await {
        log::debug!("emitting message: {:?}", msg);
        let data = serde_json::to_string(&msg)?;
        sender.send("message", data, None).await?;
    }

    Ok(())
}

async fn chat_send(mut req: tide::Request<State>) -> tide::Result {
    let body = req.body_string().await?;
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let (sess_id, sess) = req.state().sess_or_404(room_id, req.param("session")?)?;

    log::debug!("received message in {}: {:?}", room_id, body);
    req.state().send_message(room_id, sess_id, &sess, body).await?;
    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn chat_page(req: tide::Request<State>) -> tide::Result {
    let room_id_str = req.param("room")?;
    req.state().room_or_404(room_id_str)?;  // Ensure existence.

    let tera = &req.state().tera;
    tera.render_response("chat.html", &context! {
        "room_id" => room_id_str
    })
}

async fn chat_history(req: tide::Request<State>) -> tide::Result<Body> {
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let sessions = req.state().store.all_sessions(room_id)?;
    let msgs = req.state().store.iter_messages(room_id)?;
    let outgoing: Result<Vec<_>, _> = msgs.map(|r| {
        r.map(|msg| {
            OutgoingMessage::new(&msg, sessions.get(&msg.session).unwrap())
        })
    }).collect();
    Ok(Body::from_json(&outgoing?)?)
}

async fn make_chat(req: tide::Request<State>) -> tide::Result {
    let room_id = req.state().store.add_room()?;
    req.state().get_chan(room_id);  // Eagerly materialize the channel.

    // Redirect to the chat page.
    let dest = format!("/{}", req.state().fmt_id(room_id));
    Ok(tide::Redirect::new(dest).into())
}

async fn make_session(mut req: tide::Request<State>) -> tide::Result {
    let data: IncomingSession = req.body_json().await?;
    let room_id = req.state().room_or_404(req.param("room")?)?;

    let session = store::Session {
        user: data.user.to_string(),
        ts: Utc::now(),
    };
    let id = req.state().store.add_session(room_id, &session)?;
    let id_str = req.state().harsh.encode(&[id]);
    Ok(tide::Response::from(id_str))
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    log::with_level(log::LevelFilter::Debug);
    let mut app = tide::with_state(State {
        tera,
        chans: Arc::new(Mutex::new(HashMap::new())),
        store: store::Store::new("band.db")?,
        harsh: harsh::Harsh::default(),
    });

    app.at("/:room/chat").get(tide::sse::endpoint(chat_stream));
    app.at("/:room").get(chat_page);
    app.at("/:room/history").get(chat_history);
    app.at("/:room/session").post(make_session);
    app.at("/:room/session/:session/message").post(chat_send);

    app.at("/new").post(make_chat);
    app.at("/").get(|req: tide::Request<State>| async move {
        let tera = &req.state().tera;
        tera.render_response("home.html", &context! {})
    });

    app.at("/static").serve_dir("static/")?;

    app.listen("localhost:8080").await?;
    Ok(())
}

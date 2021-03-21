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

#[derive(Serialize, Deserialize, Debug)]
struct IncomingMessage {
    body: String,
    user: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct IncomingSession {
    user: String,
}

type Channel = BroadcastChannel<store::Message>;

#[derive(Clone)]
struct State {
    tera: Tera,
    chans: Arc<Mutex<HashMap<store::Id, Channel>>>,
    store: store::Store,
    harsh: harsh::Harsh,
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

    async fn send_message(&self, room_id: store::Id, incoming: IncomingMessage) -> tide::Result<()> {
        // Record message in the history database.
        let msg = store::Message {
            body: incoming.body,
            user: incoming.user,
            ts: Utc::now(),
        };
        self.store.add_message(room_id, &msg)?;

        // Send to connected clients.
        let chan = self.get_chan(room_id);
        chan.send(&msg).await?;

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
    let msg: IncomingMessage = req.body_json().await?;
    let room_id = req.state().room_or_404(req.param("room")?)?;

    log::debug!("received message in {}: {:?}", room_id, msg);
    req.state().send_message(room_id, msg).await?;
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
    let msgs = req.state().store.all_messages(room_id)?;
    Ok(Body::from_json(&msgs)?)
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

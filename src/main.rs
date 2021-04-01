use tera::Tera;
use tide::{Body, log, Response, StatusCode};
use tide_tera::prelude::*;
use broadcaster::BroadcastChannel;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use chrono::prelude::*;
use rust_embed::RustEmbed;
use std::path::Path;

mod store;

#[derive(Serialize, Debug, Clone)]
struct OutgoingMessage {
    id: String,
    body: String,
    user: String,
    votes: usize,
    ts: DateTime<Utc>,
}

#[derive(Serialize, Debug, Clone)]
struct VoteChange {
    message: String,
    delta: i8,
}

#[derive(Debug, Clone)]
enum Event {
    Message(OutgoingMessage),
    Vote(VoteChange),
}

type Channel = BroadcastChannel<Event>;

#[derive(Serialize, Deserialize, Debug)]
struct IncomingSession {
    user: String,
}

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

    fn get_session(&self, req: &tide::Request<State>, room: store::Id) -> tide::Result<Option<(store::Id, store::Session)>> {
        match req.header("Session") {
            Some(hdr) => {
                let id = self.parse_id(&hdr.as_str())?;
                Ok(self.store.get_session(room, id)?.map(|s| (id, s)))
            },
            None => Ok(None),
        }
    }

    fn require_session(&self, req: &tide::Request<State>, room: store::Id) -> tide::Result<(store::Id, store::Session)> {
        match self.get_session(&req, room)? {
            Some(v) => Ok(v),
            None => Err(tide::Error::from_str(403, "invalid session")),
        }
    }

    fn outgoing_message(&self, sess: &store::Session, id: store::Id, msg: &store::Message, votes: usize) -> OutgoingMessage {
        OutgoingMessage {
            id: self.fmt_id(id),
            body: msg.body.clone(),
            user: sess.user.clone(),
            ts: msg.ts,
            votes,
        }
    }

    async fn send_message(&self, room_id: store::Id, session_id: store::Id, session: &store::Session, body: String) -> tide::Result<()> {
        // Record message in the history database.
        let msg = store::Message {
            body,
            session: session_id,
            ts: Utc::now(),
        };
        let id = self.store.add_message(room_id, &msg)?;

        // Send to connected clients.
        let outgoing = self.outgoing_message(&session, id, &msg, 0);
        let evt = Event::Message(outgoing);
        self.get_chan(room_id).send(&evt).await?;

        Ok(())
    }

    pub fn fmt_id(&self, id: u64) -> String {
        self.harsh.encode(&[id])
    }

    pub fn parse_id(&self, id: &str) -> tide::Result<u64> {
        match self.harsh.decode(id) {
            Ok(data) => Ok(data[0]),
            Err(_) => Err(tide::Error::from_str(404, "bad id"))
        }
    }
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let mut chan = req.state().get_chan(room_id);

    while let Some(evt) = chan.next().await {
        match evt {
            Event::Message(msg) => {
                log::debug!("emitting message: {:?}", msg);
                let data = serde_json::to_string(&msg)?;
                sender.send("message", data, None).await?;
            },
            Event::Vote(vote) => {
                log::debug!("emitting vote: {:?}", vote);
                let data = serde_json::to_string(&vote)?;
                sender.send("vote", data, None).await?;
            },
        }
    }

    Ok(())
}

#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticAsset;

#[derive(RustEmbed)]
#[folder = "templates/"]
struct TemplateAsset;

async fn chat_send(mut req: tide::Request<State>) -> tide::Result {
    let body = req.body_string().await?;
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let (sess_id, sess) = req.state().require_session(&req, room_id)?;

    log::debug!("received message in {}: {:?}", room_id, body);
    req.state().send_message(room_id, sess_id, &sess, body).await?;
    Ok(Response::new(StatusCode::Ok))
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
        r.map(|(id, msg)| {
            // Error handling is not great here (complicated by closure).
            let sess = sessions.get(&msg.session).unwrap();
            let votes = req.state().store.count_votes(room_id, id).unwrap();
            req.state().outgoing_message(&sess, id, &msg, votes)
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
    Ok(Response::from(id_str))
}

async fn update_session(mut req: tide::Request<State>) -> tide::Result {
    let data: IncomingSession = req.body_json().await?;
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let (sess_id, _) = req.state().require_session(&req, room_id)?;
    req.state().store.set_user(room_id, sess_id, &data.user)?;
    Ok(Response::new(StatusCode::Ok))
}

async fn get_session(req: tide::Request<State>) -> tide::Result<Body> {
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let (_, sess) = req.state().require_session(&req, room_id)?;
    tide::Body::from_json(&sess)
}

async fn set_vote(mut req: tide::Request<State>) -> tide::Result {
    let body = req.body_string().await?;
    let vote = body.trim() != "0";

    let room_id = req.state().room_or_404(req.param("room")?)?;
    let (sess_id, _) = req.state().require_session(&req, room_id)?;
    let msg_id = req.state().parse_id(req.param("message")?)?;  // TODO 404

    // Record the vote.
    log::debug!("received vote in {}: {} for {}", room_id, vote, msg_id);
    if vote {
        req.state().store.set_vote(room_id, msg_id, sess_id)?;
    } else {
        req.state().store.reset_vote(room_id, msg_id, sess_id)?;
    }

    // Emit vote-change event.
    let evt = Event::Vote(VoteChange {
        message: req.state().fmt_id(msg_id),
        delta: if vote { 1 } else { -1 },
    });
    req.state().get_chan(room_id).send(&evt).await?;

    Ok(Response::new(StatusCode::Ok))
}

async fn get_votes(req: tide::Request<State>) -> tide::Result<Body> {
    let room_id = req.state().room_or_404(req.param("room")?)?;
    let (sess_id, _) = req.state().require_session(&req, room_id)?;

    let votes = req.state().store.iter_votes(room_id, sess_id)?;
    let vote_strs = votes.map(|r| {
        r.map(|id| req.state().fmt_id(id))
    });
    let vote_vec: Result<Vec<_>, _> = vote_strs.collect();
    tide::Body::from_json(&vote_vec?)
}

// Like Body::from_bytes, but also guesses a MIME type from the path like
// Body::from_file.
fn body_from_bytes_and_path(bytes: Vec<u8>, path: &Path) -> tide::Body {
    let mut body = tide::Body::from_bytes(bytes);

    // From http-types's guess_ext.
    let ext = path.extension().map(|p| p.to_str()).flatten();
    let m = ext.and_then(http_types::Mime::from_extension);

    match m {
        Some(mime) => body.set_mime(mime),
        None => (),
    }

    body
}

async fn static_asset(req: tide::Request<State>) -> tide::Result {
    // From tide::ServeDir.
    let path = req.url().path();
    let path = path.strip_prefix("/static").unwrap();
    let path = path.trim_start_matches('/');
    log::info!("requested static file: {:?}", path);

    match StaticAsset::get(&path) {
        Some(b) => {
            let body = body_from_bytes_and_path(b.to_vec(), Path::new(path));
            Ok(Response::builder(StatusCode::Ok).body(body).build())
        },
        None => Ok(Response::new(StatusCode::NotFound))
    }
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    // Set up templates using rust-embed.
    let mut tera = Tera::default();
    let tmpls = TemplateAsset::iter().map(|filename| {
        let tmpl = TemplateAsset::get(&filename).unwrap();
        (filename, String::from_utf8(tmpl.to_vec()).unwrap())
    });
    tera.add_raw_templates(tmpls)?;
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
    app.at("/:room/votes").get(get_votes);

    app.at("/:room/session").post(make_session);
    app.at("/:room/session").get(get_session);
    app.at("/:room/session").put(update_session);

    app.at("/:room/message").post(chat_send);
    app.at("/:room/message/:message/vote").post(set_vote);

    app.at("/new").post(make_chat);
    app.at("/").get(|req: tide::Request<State>| async move {
        let tera = &req.state().tera;
        tera.render_response("home.html", &context! {})
    });

    app.at("/static/*").get(static_asset);

    app.listen("localhost:8080").await?;
    Ok(())
}

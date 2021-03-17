use tera::Tera;
use tide::Body;
use tide_tera::prelude::*;
use broadcaster::BroadcastChannel;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use nanoid::nanoid;

type Message = String;
type Channel = BroadcastChannel<Message>;

#[derive(Clone)]
struct State {
    tera: Tera,
    chans: Arc<Mutex<HashMap<String, Channel>>>,
    db: sled::Db,
}

impl State {
    fn get_chan(&self, room_id: &str) -> tide::Result<Channel>
    {
        let chans = self.chans.lock().unwrap();
        let chan = chans.get(room_id).ok_or(
            tide::Error::from_str(404, "unknown room")
        )?;
        Ok(chan.clone())
    }
}

fn msgs_tree(db: &sled::Db, room_id: &str) -> sled::Result<sled::Tree> {
    // this could surely be made more efficient using byte manipulation instead of format!
    let tree_name = format!("msgs:{}", room_id);
    db.open_tree(tree_name)
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let room_id = req.param("room")?;
    let mut chan = req.state().get_chan(room_id)?;

    while let Some(msg) = chan.next().await {
        println!("recv'd {}", msg);
        sender.send("message", msg, None).await?;
    }

    Ok(())
}

async fn chat_send(mut req: tide::Request<State>) -> tide::Result {
    let data: String = req.body_json().await?;
    let room_id = req.param("room")?;
    println!("message in {}: {}", room_id, data);

    // Send to connected clients.
    let chan = req.state().get_chan(room_id)?;
    chan.send(&data).await?;

    // Record message in the history database.
    let db = &req.state().db;
    let msgs = msgs_tree(&db, room_id)?;
    let msg_id = db.generate_id()?.to_be_bytes();
    msgs.insert(msg_id, data.as_bytes())?;

    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn chat_page(req: tide::Request<State>) -> tide::Result {
    let room_id = req.param("room")?;

    // Make sure we stop with a 404 if the room does not exist.
    req.state().get_chan(room_id)?;

    let tera = &req.state().tera;
    tera.render_response("chat.html", &context! {
        "room_id" => room_id
    })
}

async fn chat_history(req: tide::Request<State>) -> tide::Result<Body> {
    let room_id = req.param("room")?;

    let db = &req.state().db;
    let msgs = msgs_tree(&db, room_id)?;
    let all_msgs: Result<Vec<_>, _> = msgs.iter().values().map(|r| {
        r.map(|data| String::from_utf8(data.to_vec()).unwrap())
    }).collect();

    Ok(Body::from_json(&all_msgs?)?)
}

async fn make_chat(req: tide::Request<State>) -> tide::Result {
    let room_id = nanoid!(8);
    let mut chans = req.state().chans.lock().unwrap();
    chans.insert(room_id.to_string(), BroadcastChannel::new());
    Ok(tide::Redirect::new(format!("/{}", room_id)).into())
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let db = sled::open("band.db")?;

    tide::log::start();
    let mut app = tide::with_state(State {
        tera: tera,
        chans: Arc::new(Mutex::new(HashMap::new())),
        db: db,
    });

    app.at("/:room/chat").get(tide::sse::endpoint(chat_stream));
    app.at("/:room").get(chat_page);
    app.at("/:room/send").post(chat_send);
    app.at("/:room/history").get(chat_history);

    app.at("/new").post(make_chat);
    app.at("/").get(|req: tide::Request<State>| async move {
        let tera = &req.state().tera;
        tera.render_response("home.html", &context! {})
    });

    app.at("/static").serve_dir("static/")?;

    app.listen("localhost:8080").await?;
    Ok(())
}

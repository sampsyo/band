use tera::Tera;
use tide::Body;
use tide_tera::prelude::*;
use broadcaster::BroadcastChannel;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

type Message = String;

struct Room {
    chan: BroadcastChannel<Message>,
    history: Vec<Message>,
}

#[derive(Clone)]
struct State {
    tera: Tera,
    rooms: Arc<Mutex<HashMap<String, Room>>>,
}

fn get_room<'a>(map: &'a HashMap<String, Room>, key: &str) -> tide::Result<&'a Room>
{
    map.get(key).ok_or(
        tide::Error::from_str(404, "unknown room")
    )
}

fn get_room_mut<'a>(map: &'a mut HashMap<String, Room>, key: &str) -> tide::Result<&'a mut Room>
{
    map.get_mut(key).ok_or(
        tide::Error::from_str(404, "unknown room")
    )
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let room_id = req.param("room")?;
    let mut chan = {
        let rooms = req.state().rooms.lock().unwrap();
        let room = get_room(&rooms, room_id)?;
        room.chan.clone()
    };

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
    let chan = {
        let rooms = &mut req.state().rooms.lock().unwrap();
        let room = get_room(&rooms, room_id)?;
        room.chan.clone()
    };
    chan.send(&data).await?;

    // Record message in the history.
    let rooms = &mut req.state().rooms.lock().unwrap();
    let room = &mut get_room_mut(rooms, room_id)?;
    room.history.push(data);

    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn chat_page(req: tide::Request<State>) -> tide::Result {
    let room_id = req.param("room")?;

    // Make sure we stop with a 404 if the room does not exist.
    {
        let rooms = req.state().rooms.lock().unwrap();
        get_room(&rooms, room_id)?;
    }

    let tera = &req.state().tera;
    tera.render_response("chat.html", &context! {})
}

async fn chat_history(req: tide::Request<State>) -> tide::Result<Body> {
    let room_id = req.param("room")?;

    let rooms = &req.state().rooms.lock().unwrap();
    let room = &get_room(rooms, room_id)?;

    Ok(Body::from_json(&room.history)?)
}

async fn make_chat(req: tide::Request<State>) -> tide::Result {
    let mut rooms = req.state().rooms.lock().unwrap();
    let room_id = "TODO";
    rooms.insert(room_id.to_string(), Room {
        chan: BroadcastChannel::new(),
        history: vec![],
    });
    Ok(tide::Redirect::new(format!("/{}", room_id)).into())
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();

    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let mut app = tide::with_state(State {
        tera: tera,
        rooms: Arc::new(Mutex::new(HashMap::new())),
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

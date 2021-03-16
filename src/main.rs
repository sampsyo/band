use tera::Tera;
use tide::Body;
use tide_tera::prelude::*;
use broadcaster::BroadcastChannel;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};

type Message = String;

#[derive(Clone)]
struct State {
    tera: Tera,
    chan: BroadcastChannel<Message>,
    history: Arc<Mutex<Vec<Message>>>,
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let chan = &req.state().chan;
    while let Some(msg) = chan.clone().next().await {
        println!("recv'd {}", msg);
        sender.send("message", msg, None).await?;
    }

    Ok(())
}

async fn chat_send(mut req: tide::Request<State>) -> tide::Result {
    let data: String = req.body_json().await?;
    println!("message: {}", data);

    // Send to connected clients.
    let chan = &req.state().chan;
    chan.send(&data).await?;

    // Record message in the history.
    let mut hist = req.state().history.lock().unwrap();
    hist.push(data);

    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn chat_page(req: tide::Request<State>) -> tide::Result {
    let tera = &req.state().tera;
    tera.render_response("chat.html", &context! {})
}

async fn chat_history(req: tide::Request<State>) -> tide::Result<Body> {
    let hist = req.state().history.lock().unwrap();
    Ok(Body::from_json(&*hist)?)
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();

    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let mut app = tide::with_state(State {
        tera: tera,
        chan: BroadcastChannel::new(),
        history: Arc::new(Mutex::new(vec![])),
    });

    app.at("/chat").get(tide::sse::endpoint(chat_stream));
    app.at("/").get(chat_page);
    app.at("/static").serve_dir("static/")?;
    app.at("/send").post(chat_send);
    app.at("/history").get(chat_history);

    app.listen("localhost:8080").await?;
    Ok(())
}

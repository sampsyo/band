use tera::Tera;
use tide_tera::prelude::*;
use async_std::channel;

#[derive(Clone)]
struct State {
    tera: Tera,
    sender: channel::Sender<String>,
    receiver: channel::Receiver<String>,
}

async fn chat_stream(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let receiver = &req.state().receiver;
    while let Ok(msg) = receiver.recv().await {
        println!("recv'd {}", msg);
        sender.send("message", msg, None).await?;
    }

    Ok(())
}

async fn chat_send(mut req: tide::Request<State>) -> tide::Result {
    let data: String = req.body_json().await?;
    println!("message: {}", data);

    let sender = &req.state().sender;
    sender.send(data).await?;

    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn chat_page(req: tide::Request<State>) -> tide::Result {
    let tera = &req.state().tera;
    tera.render_response("chat.html", &context! {})
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();

    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let (sender, receiver) = channel::bounded(16);
    let mut app = tide::with_state(State {
        tera: tera,
        sender: sender,
        receiver: receiver,
    });

    app.at("/chat").get(tide::sse::endpoint(chat_stream));
    app.at("/").get(chat_page);
    app.at("/static").serve_dir("static/")?;
    app.at("/send").post(chat_send);

    app.listen("localhost:8080").await?;
    Ok(())
}

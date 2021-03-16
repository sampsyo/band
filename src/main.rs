use tera::Tera;
use tide_tera::prelude::*;

#[derive(Clone)]
struct State {
    tera: Tera,
    sender: broadcast_channel::Sender<i64>,
    receiver: broadcast_channel::Receiver<i64>,
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();

    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let (sender, receiver) = broadcast_channel::broadcast(16);
    let mut app = tide::with_state(State {
        tera: tera,
        sender: sender,
        receiver: receiver,
    });

    app.at("/chat").get(tide::sse::endpoint(|req: tide::Request<State>, sender| async move {
        sender.send("message", "foo", None).await?;
        sender.send("message", "bar", None).await?;

        let receiver = &req.state().receiver;
        while let Ok(val) = receiver.recv().await {
            println!("recv'd {}", val);
        }

        Ok(())
    }));

    app.at("/").get(|req: tide::Request<State>| async move {
        let tera = &req.state().tera;
        tera.render_response("chat.html", &context! {})
    });

    app.at("/static").serve_dir("static/")?;

    app.at("/send").post(|mut req: tide::Request<State>| async move {
        let data: String = req.body_json().await?;
        println!("message: {}", data);

        let sender = &req.state().sender;
        sender.broadcast(42).await?;

        Ok("ok")
    });

    app.listen("localhost:8080").await?;
    Ok(())
}

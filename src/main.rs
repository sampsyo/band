use tera::Tera;
use tide_tera::prelude::*;

#[derive(Clone)]
struct State {
    tera: Tera,
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();

    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let mut app = tide::with_state(State {
        tera: tera,
    });

    app.at("/chat").get(tide::sse::endpoint(|req, sender| async move {
        let _state = req.state();
        sender.send("message", "foo", None).await?;
        sender.send("message", "bar", None).await?;
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
        Ok("ok")
    });

    app.listen("localhost:8080").await?;
    Ok(())
}

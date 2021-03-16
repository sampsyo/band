use tide::sse;
use tera::Tera;
use tide_tera::prelude::*;

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();

    let mut tera = Tera::new("templates/**/*")?;
    tera.autoescape_on(vec!["html"]);

    let mut app = tide::with_state(tera);

    app.at("/sse").get(sse::endpoint(|_req, sender| async move {
        sender.send("message", "foo", None).await?;
        sender.send("message", "bar", None).await?;
        Ok(())
    }));

    app.at("/").get(|req: tide::Request<Tera>| async move {
        let tera = req.state();
        tera.render_response("chat.html", &context! {})
    });

    app.listen("localhost:8080").await?;
    Ok(())
}

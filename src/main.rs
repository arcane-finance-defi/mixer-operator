use dotenv::dotenv;
use rocket::http::Method;
use rocket_cors::{AllowedOrigins, CorsOptions};
use tokio::sync::mpsc;

use crate::config::Config;
use crate::mixer::{MixClientRequest, event_loop};
use crate::api::MixerState;

mod api;
mod config;
mod mixer;

#[tokio::main] //(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_methods(
            vec![Method::Get, Method::Post, Method::Patch]
                .into_iter()
                .map(From::from)
                .collect(),
        )
        .allow_credentials(true);

    let rocket = rocket::build().attach(cors.to_cors().unwrap());

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let (sender, receiver) = mpsc::channel::<MixClientRequest>(100);

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        event_loop(config, receiver, runtime);
    });

    rocket
        .manage(MixerState { client: sender })
        .mount("/", routes![mix])
        .launch()
        .await?;

    Ok(())
}

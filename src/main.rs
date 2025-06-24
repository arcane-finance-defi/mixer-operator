use dotenv::dotenv;
use rocket::http::Method;
use rocket_cors::{AllowedOrigins, CorsOptions};
use tokio::sync::mpsc;
use tracing::info;

use mixer_operator::{
    api::{MixerState, mix_post_handler},
    mixer::{MixClientRequest, event_loop},
    config::Config,
    logging,
    PACKAGE, VERSION
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    
    logging::init();
    info!("Starting {PACKAGE}, version {VERSION}");
    
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

    // event loop for miden worker
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("async runtime");

        event_loop(config, receiver, runtime);
    });

    // main event loop for API launched by rocket
    rocket
        .manage(MixerState::new(sender))
        .mount("/", rocket::routes![mix_post_handler])
        .launch()
        .await?;

    Ok(())
}

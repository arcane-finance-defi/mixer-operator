use dotenv::dotenv;
use rocket::http::Method;
use rocket_cors::{AllowedHeaders, AllowedOrigins, CorsOptions};
use tokio::sync::mpsc;
use tracing::info;

use mixer_operator::{
    PACKAGE, VERSION,
    config::Config,
    db, logging,
    mixer::{MixClientRequest, event_loop},
    state::MixerState,
};

#[rocket::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    logging::init();
    info!("Starting {PACKAGE}, version {VERSION}");

    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .send_wildcard(true)
        .allowed_methods(
            vec![Method::Get, Method::Post, Method::Patch, Method::Put, Method::Delete, Method::Options]
                .into_iter()
                .map(From::from)
                .collect(),
        )
        .allowed_headers(AllowedHeaders::all())
        .to_cors()
        .expect("CORS build error");

    let rocket = rocket::build();

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let db_url = &config.db().url;
    // TODO: deadpool + Arc
    let db_pool = db::connect(&db_url);

    let (sender, receiver) = mpsc::channel::<MixClientRequest>(100);

    // event loop in separete async runtime for miden client
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("async runtime");

        event_loop(config, receiver, runtime);
    });

    let mixer_state = MixerState::new(sender);

    // main event loop for API launched by rocket
    mixer_operator::rocket(mixer_state, db_pool, cors)
        .launch()
        .await?;

    Ok(())
}

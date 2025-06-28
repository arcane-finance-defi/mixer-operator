use dotenv::dotenv;
use rocket::http::Method;
use rocket_cors::{AllowedOrigins, CorsOptions};
use tokio::sync::mpsc;
use tracing::info;

use mixer_operator::{
    PACKAGE, VERSION, api,
    config::Config,
    db, logging,
    mixer::{MixClientRequest, event_loop},
    state::MixerState,
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

    let db_url = &config.db().url;
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

    // main event loop for API launched by rocket
    rocket
        .manage(MixerState::new(sender))
        .manage(db_pool) // TODO: move out to MixerState?
        .mount(
            "/",
            rocket::routes![
                api::mix_post_handler,
                api::drafts::new_post_handler,
                api::drafts::get_handler,
                api::drafts::activate_post_handler,
            ],
        )
        .launch()
        .await?;

    Ok(())
}

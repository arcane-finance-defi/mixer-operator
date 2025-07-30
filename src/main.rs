use std::process::ExitCode;

use anyhow::Context as _;
use dotenv::dotenv;
use rocket::{http::Method, Build, Rocket};
use rocket_cors::{AllowedHeaders, AllowedOrigins, CorsOptions};
use tokio::sync::mpsc;
use tracing::info;

use mixer_operator::{
    api, config::Config, db, executor, logging, mixer::{event_loop, MixClientRequest}, state::MixerState, PACKAGE, VERSION
};

fn rocket(mixer_state: MixerState, note_repo: impl db::NoteRepository) -> Rocket<Build> {
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

    let rocket: rocket::Rocket<rocket::Build> =
        rocket::build().attach(cors);

    rocket
        .manage(mixer_state)
        .manage(db_pool) // TODO: move out to NoteStorage?
        // legacy api
        .mount(
            "/",
            rocket::routes![
                api::mix_post_handler, // Mounting /mix
            ],
        )
        // new api
        .mount(
            "/api/v1/",
            rocket::routes![
                api::mix_post_handler,
                api::note_drafts::post_new_handler,
                api::note_drafts::get_handler,
                api::note_drafts::get_by_id_handler,
                api::note_drafts::post_activate_by_id_handler,
                api::note_drafts::delete_by_id_handler,
            ],
        )
}

#[rocket::main]
async fn main() -> anyhow::Result<ExitCode> {
    setup_panic_hook();
    dotenvy::dotenvy().ok();

    logging::init();
    info!("Starting {PACKAGE}, version {VERSION}");

    let config = rocket::Config::figment()
        .extract::<Config>()
        .context("reading figment provided config")?;

    // Prepare sidecar futures
    let cancellation_token = CancellationToken::new();
    let mut handles = FuturesUnordered::new();

    let db_url = &config.db().url;
    let db_pool = db::connect(&db_url)?;

    let (sender, receiver) = mpsc::channel::<MixClientRequest>(100);

    // Event loop in separete async runtime for Miden client (not Send'able)
    let mixer_worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("async runtime");

        event_loop(config, receiver, runtime, cancellation_token.clone());
    });

    // Note executor task
    handles.push(executor::spawn(sender.clone(), cancellation_token.clone()));

    let mixer_state = MixerState::new(sender);

    // Main event loop for API launched by rocket
    rocket(mixer_state, db_pool).launch().await?;

    // At this point the server shut down (launch result is Ok)
    // So do graceful shutdown of other features
    logging::info("Shutting down {PACKAGE}");
    cancellation_token.cancel();

    let mut exit_code = ExitCode::SUCCESS;
    while let Some((name, result)) = handles.next().await {
        if let Err(error) = result.with_context(|| format!("running {name}")) {
            exit_code = ExitCode::FAILURE;
            log::error!("Fatal error: {error:#}. Stopping");
        }
    }

    mixer_worker.join()?;

    Ok(())
}

fn setup_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::thread::sleep(Duration::from_millis(100));
        std::process::exit(1);
    }));
}

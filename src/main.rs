use std::{process::ExitCode, sync::Arc};

use anyhow::Context as _;
use fang::AsyncQueue;
use mixer_operator::{
    PACKAGE, VERSION, api,
    config::Config,
    db, logging,
    mixer::{MixClientRequest, event_loop},
    state::MixerState,
    task::worker::prepare_task_queue,
};
use rocket::{Build, Rocket, http::Method};
use rocket_cors::{AllowedHeaders, AllowedOrigins, CorsOptions};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

fn rocket(
    mixer_state: MixerState,
    note_repo: Arc<dyn db::models::NoteRepository>,
    task_queue: Arc<AsyncQueue>,
) -> Rocket<Build> {
    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .send_wildcard(true)
        .allowed_methods(
            vec![
                Method::Get,
                Method::Post,
                Method::Patch,
                Method::Put,
                Method::Delete,
                Method::Options,
            ]
            .into_iter()
            .map(From::from)
            .collect(),
        )
        .allowed_headers(AllowedHeaders::all())
        .to_cors()
        .expect("CORS build error");

    let rocket: rocket::Rocket<rocket::Build> = rocket::build().attach(cors);

    rocket
        // share state
        .manage(mixer_state)
        .manage(note_repo)
        .manage(task_queue)
        // legacy api
        .mount(
            "/",
            rocket::routes![
                api::mix::post_handler,
            ],
        )
        // new api
        .mount(
            "/api/v1/",
            api::routes(),
        )
}

#[rocket::main]
async fn main() -> anyhow::Result<ExitCode> {
    setup_panic_hook();
    dotenvy::dotenv().ok();

    logging::init();
    info!("Starting {PACKAGE}, version {VERSION}");

    let cancellation_token = CancellationToken::new();

    // Destructuring config
    let config = rocket::Config::figment()
        .extract::<Config>()
        .context("reading figment provided config")?;

    if cfg!(debug_assertions) {
        tracing::info!("Loaded config:\n{config:#?}");
    }

    let tq_config = config.task_queue();
    let db_config = config.db();
    let client_config = config.client();
    let debug = config.debug();

    // Need initialize db pool to use it by db::DatabaseStorage::storage()
    db::set_pool_url(db_config.url.clone())?;

    // Channel to interact with internal Miden client
    let (sender, receiver) =
        mpsc::channel::<MixClientRequest>(client_config.internal_queue_size() as usize);

    let stop_token = cancellation_token.clone();
    let mixer_config = client_config.clone();

    // Event loop in separete async runtime for Miden client (not Send'able)
    let mixer_worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("async runtime");

        event_loop(mixer_config, debug, receiver, runtime, stop_token);
    });

    // Task queue
    let task_queue = prepare_task_queue(tq_config, sender.clone())
        .await
        .with_context(|| "prepare task queue")?;
    let task_queue = Arc::new(task_queue);

    // Main event loop for API launched by rocket
    let storage = db::DatabaseStorage::note_storage().await.expect("rocket storage initialized");
    rocket(MixerState::new(sender.clone()), storage, task_queue.clone())
        .launch()
        .await?;

    // At this point the server shut down (launch result is Ok)
    // So do graceful shutdown of other features
    tracing::info!("Shutting down {PACKAGE}");
    cancellation_token.cancel();

    mixer_worker.join().expect("mixer thread finished");

    Ok(ExitCode::SUCCESS)
}

fn setup_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::process::exit(1);
    }));
}

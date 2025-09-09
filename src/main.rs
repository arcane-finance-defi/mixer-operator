use std::{process::ExitCode, sync::Arc};

use anyhow::Context as _;
use fang::AsyncQueue;
use futures::{StreamExt as _, stream::FuturesUnordered};
use mixer_operator::{
    api, config::Config, db, executor, logging, mixer::{event_loop, MixClientRequest}, state::MixerState, task::worker::prepare_task_queue, PACKAGE, VERSION
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
            rocket::routes![
                api::mix::post_handler,
                api::mix::delayed_post_handler,
                api::mix::delayed_status_get_handler,
                api::note_drafts::post_new_handler,
                api::note_drafts::get_status_handler,
                // api::note_drafts::get_by_id_handler,
                // api::note_drafts::post_activate_by_id_handler,
                // api::note_drafts::delete_by_id_handler,
            ],
        )
    // swagger
    // .mount(
    //     "/swagger-ui/",
    //     make_swagger_ui(&SwaggerUIConfig {
    //         url: "../openapi.json".to_owned(),
    //         ..Default::default()
    //     }),
    // )
}

#[rocket::main]
async fn main() -> anyhow::Result<ExitCode> {
    setup_panic_hook();
    dotenvy::dotenv().ok();

    logging::init();
    info!("Starting {PACKAGE}, version {VERSION}");

    let cancellation_token = CancellationToken::new();

    let config = rocket::Config::figment()
        .extract::<Config>()
        .context("reading figment provided config")?;

    // Need initialize db pool to use it by db::DatabaseStorage::storage()
    db::set_pool_url(config.db().url)?;

    // Event loop in separete async runtime for Miden client (not Send'able)
    let (sender, receiver) = mpsc::channel::<MixClientRequest>(config.client_count() as usize);
    let mixer_worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("async runtime");

        event_loop(config, receiver, runtime, cancellation_token.clone());
    });

    // Task queue
    let task_queue = prepare_task_queue(config.task_queue(), sender.clone())
        .await
        .with_context(|| "prepare task queue")?;
    let task_queue = Arc::new(task_queue);

    // Prepare sidecar futures
    let mut handles = FuturesUnordered::new();

    // Note executor task
    handles.push(executor::spawn(
        sender.clone(),
        *db::DatabaseStorage::note_storage().await.expect("executor storage initialized"),
        cancellation_token.clone(),
    ));

    // Main event loop for API launched by rocket
    rocket(
        MixerState::new(sender.clone()),
        Arc::new(db::DatabaseStorage::note_storage()),
        task_queue.clone(),
    )
    .launch()
    .await?;

    // At this point the server shut down (launch result is Ok)
    // So do graceful shutdown of other features
    tracing::info!("Shutting down {PACKAGE}");
    cancellation_token.cancel();

    let mut exit_code = ExitCode::SUCCESS;
    while let Some((name, result)) = handles.next().await {
        if let Err(error) = result.with_context(|| format!("running {name}")) {
            exit_code = ExitCode::FAILURE;
            tracing::error!("Fatal error: {error:#}. Stopping");
        }
    }

    mixer_worker.join().expect("mixer thread finished");

    Ok(exit_code)
}

fn setup_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::process::exit(1);
    }));
}

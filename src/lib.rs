use rocket::{Build, Rocket};

pub mod api;
pub mod config;
pub mod db;
pub mod logging;
pub mod mixer;
pub mod state;
mod test;

use crate::db::Pool;
use crate::state::MixerState;

pub const PACKAGE: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn rocket(mixer_state: MixerState, db_pool: Pool) -> Rocket<Build> {
    rocket::build()
    .manage(mixer_state)
    .manage(db_pool) // TODO: move out to NoteStorage?
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

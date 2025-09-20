mod error;
pub mod mix;
pub mod note_drafts;

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![
        mix::post_handler,
        mix::post_batch_handler,
        mix::delayed_post_handler,
        mix::delayed_post_batch_handler,
        mix::delayed_status_get_handler,
        note_drafts::post_new_handler,
        note_drafts::get_status_handler,
        // api::note_drafts::get_by_id_handler,
        // api::note_drafts::post_activate_by_id_handler,
        // api::note_drafts::delete_by_id_handler,
    ]
}

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket::{State, get, post};

#[post("/drafts/new", data = "<note>")]
pub async fn new_post_handler(
    note: Json<&str>, //Json<Note>,
                      // state: &State<NotesState>,
) -> (Status, String) {
    // let id = uuid::Uuid::new_v4().to_string();
    // state.drafts.insert(id.clone(), note_draft.into_inner());
    (Status::Created, "Not implemented new".to_string())
}

#[get("/drafts")]
pub async fn get_handler(/*state: &State<NotesState>*/) -> Json<String> {
    //Vec<NoteDraft>> {

    // let drafts = state.drafts.values().cloned().collect();
    Json("Not implemented get".to_string())
}

#[post("/drafts/activate/<id>")]
pub async fn activate_post_handler(
    id: u64,
    // state: &State<NotesState>
) -> Result<&'static str, Status> {
    if id < 10 {
        Ok("Not implemented activate")
    } else {
        Err(Status::NotFound)
    }
}

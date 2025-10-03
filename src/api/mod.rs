mod error;
pub mod mix;
pub mod note_drafts;

pub enum RouterMode {
    Native,
    WithSwagger,
}

macro_rules! make_routes {
    ($mode:expr, [$($route:expr,)*]) => {
        match $mode {
            RouterMode::Native => rocket::routes![$($route,)*],
            RouterMode::WithSwagger => rocket_okapi::openapi_get_routes![$($route,)*].into(),
        }
    };
}

pub fn routes(mode: RouterMode) -> Vec<rocket::Route> { 
    // this macro can cause Rust Analyzer errors
    // refer https://github.com/GREsau/okapi/issues/166
    make_routes!(
        mode, 
        [
            mix::post_handler,
            mix::post_batch_handler,
            mix::delayed_post_handler,
            mix::delayed_post_batch_handler,
            mix::delayed_status_get_handler,
            note_drafts::post_new_handler,
            note_drafts::get_status_handler,
        ]
    )
    // disabled
    // api::note_drafts::get_by_id_handler,
    // api::note_drafts::post_activate_by_id_handler,
    // api::note_drafts::delete_by_id_handler,
}

pub fn swagger() -> Vec<rocket::Route> {
    use rocket_okapi::swagger_ui::{ make_swagger_ui, SwaggerUIConfig };

    // points at /api/v1 endpoints with relative path from swagger-ui mount point
    make_swagger_ui(&SwaggerUIConfig {
        url: "../../../api/v1/openapi.json".to_owned(),
        ..Default::default()
    }).into()
}
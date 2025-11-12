use rocket::get;
use rocket_okapi::openapi;

use crate::{PACKAGE, VERSION};

#[openapi]
#[get("/healthcheck")]
#[tracing::instrument]
pub async fn healthcheck_get_handler() -> String {
    format!("{PACKAGE}:{VERSION}")
}

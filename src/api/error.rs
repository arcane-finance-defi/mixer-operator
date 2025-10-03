use hex::FromHexError;
use miden_objects::{AccountIdError, utils::DeserializationError};
use rocket::{
    http::Status,
    response,
    serde::json::{Json, json},
};
use rocket_okapi::{
    okapi::{self},
    response::OpenApiResponderInner,
};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::mixer::{MixClientRequest, client::MixerClientError};

#[derive(Debug, Error)]
pub enum EndpointError {
    #[error(transparent)]
    // #[serde(with = "FromHexErrorDef")]
    FromHex(#[from] FromHexError),
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    AccountId(#[from] AccountIdError),
    #[error(transparent)]
    MpscSend(#[from] Box<mpsc::error::SendError<MixClientRequest>>),
    #[error(transparent)]
    OneshotRecv(#[from] oneshot::error::RecvError),
    #[error(transparent)]
    MixerClient(#[from] Box<MixerClientError>),
    #[error("Internal storage error")]
    InternalStorage(String),
    #[error("Task queue error")]
    TaskQueue(String),
    #[error("Note not found")]
    NoteNotFound(String),
    #[error("Batch size exceeds the limit")]
    BatchLimit,
    #[error("Unknown source error")]
    Unknown { source: anyhow::Error },
}

impl From<anyhow::Error> for EndpointError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast::<EndpointError>() {
            Ok(inner) => inner,
            Err(other) => EndpointError::Unknown { source: other },
        }
    }
}

// https://stuarth.github.io/rocket-error-handling/
impl<'r, 'o: 'r> response::Responder<'r, 'o> for EndpointError {
    fn respond_to(self, req: &'r rocket::Request<'_>) -> response::Result<'o> {
        // log `self` to your favored error tracker, e.g.
        // sentry::capture_error(&self);

        match self {
            EndpointError::FromHex(err) => {
                let error_message =
                    Json(json!({"error": format!("FromHex error occurred - {err}")}));
                response::status::Custom(Status::BadRequest, error_message).respond_to(req)
            },
            EndpointError::AccountId(err) => {
                let error_message =
                    Json(json!({"error": format!("AccountId error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::MpscSend(err) => {
                let error_message =
                    Json(json!({"error": format!("Mixer client request error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::OneshotRecv(err) => {
                let error_message =
                    Json(json!({"error": format!("Mixer client response error - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::Deserialization(err) => {
                let error_message =
                    Json(json!({"error": format!("Deserialization error occurred - {err}")}));
                response::status::Custom(Status::UnprocessableEntity, error_message).respond_to(req)
            },
            EndpointError::MixerClient(err) => {
                let error_message =
                    Json(json!({"error": format!("Mixer Client error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::TaskQueue(err) => {
                let error_message =
                    Json(json!({"error": format!("Task Queue error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::InternalStorage(err) => {
                let error_message =
                    Json(json!({"error": format!("Internal State error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::NoteNotFound(_) => Status::NotFound.respond_to(req),
            EndpointError::BatchLimit => {
                let error_message =
                    Json(json!({"error": format!("{}", EndpointError::BatchLimit.to_string())}));
                response::status::Custom(Status::BadRequest, error_message).respond_to(req)
            },
            EndpointError::Unknown { source } => {
                let error_message =
                    Json(json!({"error": format!("An unknown error occurred - {source}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
        }
    }
}

impl From<fang::AsyncQueueError> for EndpointError {
    fn from(qerr: fang::AsyncQueueError) -> Self {
        tracing::error!("AsyncQueueError occured: {qerr:#?}");
        EndpointError::TaskQueue(qerr.to_string())
    }
}

impl From<crate::db::models::NoteRepositoryError> for EndpointError {
    fn from(derr: crate::db::models::NoteRepositoryError) -> Self {
        use crate::db::models::NoteRepositoryError;
        match derr {
            NoteRepositoryError::NotFound(smth) => EndpointError::NoteNotFound(smth),
            NoteRepositoryError::MoreThanOneRowAffected => {
                EndpointError::InternalStorage(derr.to_string())
            },
            NoteRepositoryError::Internal(e) => EndpointError::InternalStorage(e.to_string()),
            NoteRepositoryError::InteractDeadpool(s) => EndpointError::InternalStorage(s),
        }
    }
}

impl OpenApiResponderInner for EndpointError {
    fn responses(
        _generator: &mut rocket_okapi::r#gen::OpenApiGenerator,
    ) -> rocket_okapi::Result<rocket_okapi::okapi::openapi3::Responses> {
        let mut responses = rocket_okapi::okapi::openapi3::Responses::default();
        // TODO(kochetkov): if one implemented JsonSchema and/or Serialize for EndpointError
        // TODO(kochetkov): or convert it to some new type which implemented, then we would have use
        // json_schema e.g.
        // let schema = generator.json_schema::<EndpointError>();
        // rocket_okapi::util::add_schema_response(&mut responses, 400, "text/plain",
        // schema.clone())?;
        add_400_error(&mut responses);
        add_404_error(&mut responses);
        add_422_error(&mut responses);
        add_500_error(&mut responses);
        Ok(responses)
    }
}

fn add_400_error(responses: &mut okapi::openapi3::Responses) {
    responses
        .responses
        .entry("400".to_owned())
        .or_insert_with(|| {
            let response = okapi::openapi3::Response{
                description: "\
                # [400 Bad Request](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/400)\n\
                The request given is wrongly formatted or data asked could not be fulfilled.\n\n\
                This is most likely because of the `filter_by`, `order_by` or `group_by` parameters. \
                The properties you are asking for do not exist or does not allow filtering or ordering. \
                "
                .to_owned(),
                ..Default::default()
            };
            response.into()
        });
}

fn add_404_error(responses: &mut okapi::openapi3::Responses) {
    responses.responses.entry("404".to_owned())
        .or_insert_with(|| {
            let response = okapi::openapi3::Response{
                description: "\
                # [404 Not Found](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/404)\n\
                This response is given when you request a page that does not exists.\n\n\
                **Note:** This is not exactly a response by this endpoint. But might returned when \
                you wrongly input one or more of the path or query parameters. An example would be \
                that the server expects and `int32` and you have given it \"100m\", which is a `string` \
                because of the `m` character.\n\n\
                So when you get this error and you expect a result. Check all the types of the parameters. \
                ".to_owned(),
                ..Default::default()
            };
            response.into()
        });
}

fn add_422_error(responses: &mut okapi::openapi3::Responses) {
    responses.responses.entry("422".to_owned())
        .or_insert_with(|| {
            let response = okapi::openapi3::Response{
                description: "\
                # [422 Unprocessable Entity](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/422)\n\
                This response is given when you request the server to process data which is valid, \
                but unable to be processed. This can happen when a request's JSON is well-formed, \
                but contains semantically incorrect data. \n\n\
                **Note:** This is not exactly a response by this endpoint. But might be returned \
                when you wrongly input one or more of the path or query parameters, such as \
                providing an `int32` where a `string` is expected. \n\n\
                So when you get this error and you expect a result. Check all the types of the parameters. \
                ".to_owned(),
                ..Default::default()
            };
            response.into()
        });
}

fn add_500_error(responses: &mut okapi::openapi3::Responses) {
    responses.responses.entry("500".to_owned()).or_insert_with(|| {
        let response = okapi::openapi3::Response {
            description: "\
                    # [500 Internal Server Error]\
                    (https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/500)\n\
                    This response is given when the server has an internal error that it could not \
                    recover from.\n\n\
                    If you get this response please report this issue.\
                    "
            .to_owned(),
            ..Default::default()
        };
        response.into()
    });
}

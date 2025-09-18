use diesel::prelude::*;
use notes::{FullNote, NoteStatus};
use thiserror::Error;

use super::{DatabaseStorage, schema};

pub mod notes;

#[derive(Error, Debug)]
pub enum NoteRepositoryError {
    #[error("More than one rows affected")]
    MoreThanOneRowAffected,
    #[error("Note not found")]
    NotFound(String),
    #[error(transparent)]
    Internal(#[from] NoteRepositoryErrorGeneric),
}

#[derive(Debug)]
pub struct NoteRepositoryErrorGeneric {
    inner: Box<dyn std::error::Error>,
}

impl std::fmt::Display for NoteRepositoryErrorGeneric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.to_string();
        write!(f, "internal note repository error {inner}")
    }
}

impl std::error::Error for NoteRepositoryErrorGeneric {}

// This enables using `?` on functions that return `Result<_, Error>` to turn them into
// `Result<_, NoteRepositoryError>`. That way you don't need to do that manually.
// impl<E> From<E> for NoteRepositoryError
// where
//     E: Into<Box<dyn std::error::Error>>,
// {
//     #[track_caller]
//     fn from(err: E) -> Self {
//         Self { inner: err.into() }
//     }
// }
impl NoteRepositoryErrorGeneric {
    pub fn new<E>(err: E) -> Self
    where
        E: Into<Box<dyn std::error::Error>>,
    {
        NoteRepositoryErrorGeneric { inner: err.into() }
    }
}

#[async_trait::async_trait]
pub trait NoteRepository: Send + Sync {
    async fn add_note(&self, note: notes::FullNote) -> Result<(), NoteRepositoryError>;

    async fn get_note_by_id(&self, note_id: &str) -> Result<FullNote, NoteRepositoryError>;

    async fn get_note_by_request_id(&self, req_id: &str) -> Result<FullNote, NoteRepositoryError>;

    async fn get_note_status_by_id(&self, note_id: &str)
    -> Result<NoteStatus, NoteRepositoryError>;

    async fn update_note_status_by_id(
        &self,
        note_id: &str,
        new_status: NoteStatus,
    ) -> Result<(), NoteRepositoryError>;

    async fn get_notes_by_status(
        &self,
        req_status: NoteStatus,
    ) -> Result<Vec<FullNote>, NoteRepositoryError>;
}

#[async_trait::async_trait]
impl NoteRepository for DatabaseStorage {
    async fn add_note(&self, note: FullNote) -> Result<(), NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        let result = conn
            .interact(move |conn| {
                diesel::insert_into(schema::notes::table).values(&note).execute(conn)
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        if result != 1 {
            return Err(NoteRepositoryError::MoreThanOneRowAffected);
        }
        Ok(())
    }

    async fn get_note_by_id(&self, note_id: &str) -> Result<FullNote, NoteRepositoryError> {
        let find_note_id = note_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        let result = conn
            .interact(|conn| {
                schema::notes::table
                    .filter(schema::notes::note_id.eq(find_note_id))
                    .first::<FullNote>(conn)
                    .optional()
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        Ok(result.ok_or_else(|| NoteRepositoryError::NotFound(note_id.to_string()))?)
    }

    async fn get_note_by_request_id(&self, req_id: &str) -> Result<FullNote, NoteRepositoryError> {
        let find_req_id = req_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        let result = conn
            .interact(|conn| {
                schema::notes::table
                    .filter(schema::notes::request_id.eq(find_req_id))
                    .first::<FullNote>(conn)
                    .optional()
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        Ok(result.ok_or_else(|| NoteRepositoryError::NotFound(req_id.to_string()))?)
    }

    async fn get_note_status_by_id(
        &self,
        note_id: &str,
    ) -> Result<NoteStatus, NoteRepositoryError> {
        let find_note_id = note_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        let result = conn
            .interact(|conn| {
                schema::notes::table
                    .filter(schema::notes::note_id.eq(find_note_id))
                    .first::<FullNote>(conn)
                    .optional()
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        let result = result.ok_or_else(|| NoteRepositoryError::NotFound(note_id.to_string()))?;
        Ok(result.status)
    }

    async fn update_note_status_by_id(
        &self,
        find_note_id: &str,
        new_status: NoteStatus,
    ) -> Result<(), NoteRepositoryError> {
        let find_note_id = find_note_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        use crate::db::schema::notes::dsl::*;
        let result = conn
            .interact(move |conn| {
                diesel::update(notes.filter(note_id.eq(find_note_id)))
                    .set(status.eq(new_status))
                    .execute(conn)
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        if result != 1 {
            return Err(NoteRepositoryError::MoreThanOneRowAffected);
        }
        Ok(())
    }

    async fn get_notes_by_status(
        &self,
        req_status: NoteStatus,
    ) -> Result<Vec<FullNote>, NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        use diesel::{
            dsl::sql,
            sql_types::{Bool, Integer},
        };
        let result = conn
            .interact(move |conn| {
                schema::notes::table
                    .filter(
                        sql::<Bool>("status & ")
                            .bind::<Integer, _>(req_status)
                            .sql(" = ")
                            .bind::<Integer, _>(req_status),
                    )
                    .load::<FullNote>(conn)
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        Ok(result)
    }
}

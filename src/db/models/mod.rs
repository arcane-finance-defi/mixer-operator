use chrono::{DateTime, Utc};
use diesel::{prelude::*, result::Error::NotFound};
use notes::{FullNote, NoteStatus};
use thiserror::Error;

use crate::db::models::notes::NoteDetails;

use super::{DatabaseStorage, schema};

pub mod notes;

#[derive(Error, Debug)]
pub enum NoteRepositoryError {
    #[error("More than one rows affected")]
    MoreThanOneRowAffected,
    #[error("Note not found")]
    NotFound(String),
    #[error("Pool interact error")]
    InteractDeadpool(String),
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

impl From<diesel::result::Error> for NoteRepositoryError {
    fn from(error: diesel::result::Error) -> Self {
        match error {
            // TODO:
            // diesel::result::Error::InvalidCString(nul_error) => todo!(),
            // diesel::result::Error::DatabaseError(database_error_kind, database_error_information) => todo!(),
            diesel::result::Error::NotFound => NoteRepositoryError::NotFound(String::new()),
            // diesel::result::Error::QueryBuilderError(error) => todo!(),
            // diesel::result::Error::DeserializationError(error) => todo!(),
            // diesel::result::Error::SerializationError(error) => todo!(),
            // diesel::result::Error::RollbackErrorOnCommit { rollback_error, commit_error } => todo!(),
            // diesel::result::Error::RollbackTransaction => todo!(),
            // diesel::result::Error::AlreadyInTransaction => todo!(),
            // diesel::result::Error::NotInTransaction => todo!(),
            // diesel::result::Error::BrokenTransactionManager => todo!(),
            err => NoteRepositoryError::Internal(NoteRepositoryErrorGeneric::new(err)),
        }
    }
}

impl From<deadpool_diesel::InteractError> for NoteRepositoryError {
    fn from(error: deadpool_diesel::InteractError) -> Self {
        use deadpool_diesel::InteractError;
        match error {
            InteractError::Panic(erased_type) => 
                NoteRepositoryError::InteractDeadpool(format!("pool: {}", InteractError::Panic(erased_type))),
            InteractError::Aborted => 
                NoteRepositoryError::InteractDeadpool(format!("pool: {}", InteractError::Aborted)),
        }
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

    async fn get_note_status_by_ids(&self, note_ids: Vec<String>)
    -> Result<Vec<NoteStatus>, NoteRepositoryError>;

    async fn update_note_status_by_id(
        &self,
        note_id: &str,
        new_status: NoteStatus,
    ) -> Result<(), NoteRepositoryError>;

    async fn update_note_status_by_ids(
        &self,
        note_id_statuses: Vec<(String, NoteStatus)>,
    ) -> Result<(), NoteRepositoryError>;

    /// Retrieves `FullNote`s from repository by status bitmask
    async fn get_notes_by_status(
        &self,
        req_status: NoteStatus,
    ) -> Result<Vec<FullNote>, NoteRepositoryError>;

    /// Retrieves `FullNote`s from repository by status bitmask and scheduled date <= `date`
    async fn get_notes_by_status_and_date(
        &self,
        req_status: NoteStatus,
        date: DateTime<Utc>,
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

    /// Get notes statuses in one transaction
    /// Resulting statuses vector is guaranteed to be in the same order as input `note_ids` 
    async fn get_note_status_by_ids(&self, note_ids: Vec<String>)
    -> Result<Vec<NoteStatus>, NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        let result = conn
            .interact(|conn| {
                conn.transaction(|conn| {
                
                let mut statuses: Vec<NoteStatus> = Vec::with_capacity(note_ids.len());

                for note_id in note_ids {
                    // simply return NotFound error if not exists in the table
                    let note = schema::notes::table
                        .select(NoteDetails::as_select())
                        .filter(schema::notes::note_id.eq(note_id))
                        .first::<NoteDetails>(conn)?;
                    statuses.push(note.status);
                }
                
                // That uses `diesel::result::Error` as error type
                // you can use any other error type here as long as
                // it implements `From<diesel::result::Error>`.
                diesel::result::QueryResult::Ok(statuses)
            })
        })
        .await
        // this maps the deadpool interact errors
        .map_err(NoteRepositoryError::from)?
        // and this maps the diesel error
        .map_err(NoteRepositoryError::from)?;
        
        Ok(result)
    }

    async fn update_note_status_by_ids(
        &self,
        note_id_statuses: Vec<(String, NoteStatus)>,
    ) -> Result<(), NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        use diesel::result::{QueryResult, Error};
        use crate::db::schema::notes::dsl::{notes as db_notes, status as db_status, note_id as db_note_id};
        let result = conn
            .interact(|conn| {
                conn.transaction(|conn| {

                for (new_note_id, new_status) in note_id_statuses {
                    let rows_affected = diesel::update(db_notes.filter(db_note_id.eq(new_note_id)))
                        .set(db_status.eq(new_status))
                        .execute(conn)?;
                    if rows_affected != 1 {
                        // TODO: should return actual error, not generic RollbackTransaction 
                        return QueryResult::Err(Error::RollbackTransaction);
                    }
                }
                QueryResult::Ok(())
            })
        })
        .await
        .map_err(NoteRepositoryError::from)?
        .map_err(NoteRepositoryError::from)?;
        
        Ok(result)
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

    async fn get_notes_by_status_and_date(
        &self,
        req_status: NoteStatus,
        date: DateTime<Utc>,
    ) -> Result<Vec<FullNote>, NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryErrorGeneric::new)?;

        use diesel::{
            dsl::sql,
            sql_types::{Bool, Integer},
        };
        use crate::db::schema::notes::dsl as schema_dsl;
        let result = conn
            .interact(move |conn| {
                schema::notes::table
                    .filter(
                        sql::<Bool>("status & ")
                            .bind::<Integer, _>(req_status)
                            .sql(" = ")
                            .bind::<Integer, _>(req_status),
                    )
                    .filter(schema_dsl::scheduled_datetime.le(date.naive_utc()))
                    .load::<FullNote>(conn)
            })
            .await
            .map_err(NoteRepositoryErrorGeneric::new)?
            .map_err(NoteRepositoryErrorGeneric::new)?;

        Ok(result)
    }
}

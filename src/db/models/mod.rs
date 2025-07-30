use std::{ffi::OsString, sync::Arc};

use anyhow::{anyhow, bail, Context};
use diesel::{prelude::*, sql_query};
use notes::{FullNote, NoteStatus};
use thiserror::Error;
use tracing_subscriber::fmt::format::Full;

use super::{schema, DbConnection, DbPool};

pub mod notes;

pub struct DatabaseStorage {
    pool: DbPool,
}

impl DatabaseStorage {
    pub fn new(pool: DbPool) -> Self {
        DatabaseStorage { pool }
    }
}

#[derive(Error, Debug)]
enum NoteRepositoryError {
    #[error("More than one rows affected")]
    MoreThanOneRowAffected,
    #[error("Note not found")]
    NotFound,
}

pub struct NoteRepositoryErrorGeneric {
    inner: Box<dyn std::error::Error>,
}

// This enables using `?` on functions that return `Result<_, Error>` to turn them into
// `Result<_, NoteRepositoryError>`. That way you don't need to do that manually.
impl<E> From<E> for NoteRepositoryErrorGeneric
where
    E: Into<Box<dyn std::error::Error>>,
{
    #[track_caller]
    fn from(err: E) -> Self {
        Self {
            inner: err.into(),
        }
    }
}

pub trait NoteRepository {
    async fn add_note(&self, note: notes::FullNote) -> Result<(), NoteRepositoryErrorGeneric>;
    async fn get_note_by_id(&self, note_id: &str) -> Result<FullNote, NoteRepositoryErrorGeneric>;
    async fn get_note_status_by_id(&self, note_id: &str) -> Result<NoteStatus, NoteRepositoryErrorGeneric>;
    async fn update_note_status_by_id(&self, note_id: &str, new_status: NoteStatus) -> Result<(), NoteRepositoryErrorGeneric>;
    async fn get_notes_by_status(&self, req_status: NoteStatus) -> Result<Vec<FullNote>, NoteRepositoryErrorGeneric>;
}

impl NoteRepository for DatabaseStorage {
    async fn add_note(&self, note: FullNote) -> Result<(), NoteRepositoryErrorGeneric> {
        let conn = self.pool.get().await?;

        let result = conn.interact(
            move |conn| {
                diesel::insert_into(schema::notes::table)
                    .values(&note)
                    .execute(conn)
            }
        ).await??;

        if result != 1 {
            return Err(NoteRepositoryError::MoreThanOneRowAffected.into());
        }
        Ok(())
    }

    async fn get_note_by_id(&self, note_id: &str) -> Result<FullNote, NoteRepositoryErrorGeneric> {
        let find_note_id = note_id.to_string();

        let conn = self.pool.get().await?;

        let result = conn.interact(
            |conn| {
                schema::notes::table
                    .filter(schema::notes::note_id.eq(find_note_id))
                    .first::<FullNote>(conn)
                    .optional()
            }
        ).await??;
        
        Ok(result.ok_or_else(|| NoteRepositoryError::NotFound)?)
    }

    async fn get_note_status_by_id(&self, note_id: &str) -> Result<NoteStatus, NoteRepositoryErrorGeneric> {
        let find_note_id = note_id.to_string();

        let conn = self.pool.get().await?;
        
        let result = conn.interact(
            |conn| {
                schema::notes::table
                    .filter(schema::notes::note_id.eq(find_note_id))
                    .first::<FullNote>(conn)
                    .optional()
            }
        ).await??;
        
        let result = result.ok_or_else(|| anyhow!(NoteRepositoryError::NotFound))?;
        Ok(result.status)
    }

    async fn update_note_status_by_id(&self, find_note_id: &str, new_status: NoteStatus) -> Result<(), NoteRepositoryErrorGeneric> {
        let find_note_id = find_note_id.to_string();      

        let conn = self.pool.get().await?;

        use crate::db::schema::notes::dsl::*;
        let result = conn.interact(
            move |conn| {
                diesel::update(notes.filter(note_id.eq(find_note_id)))
                    .set(status.eq(new_status))
                    .execute(conn)
            }
        ).await??;
        
        if result != 1 {
            return Err(NoteRepositoryError::MoreThanOneRowAffected.into());
        }
        Ok(())
    }

    async fn get_notes_by_status(&self, req_status: NoteStatus) -> Result<Vec<FullNote>, NoteRepositoryErrorGeneric> {
        let conn = self.pool.get().await?;

        use diesel::dsl::sql;
        use diesel::sql_types::{Bool, Integer};
        let result = conn.interact(
            move |conn| {
                schema::notes::table
                    .filter(
                        sql::<Bool>("status & ")
                        .bind::<Integer, _>(req_status)
                        .sql(" = ")
                        .bind::<Integer, _>(req_status)
                    )
                    .load::<FullNote>(conn)
            }
        ).await??;
        
        Ok(result)
    }
}

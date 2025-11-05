use anyhow::{Context as _, anyhow};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use notes::{FullNote, NoteStatus};
use thiserror::Error;

use super::{DatabaseStorage, schema};
use crate::db::models::notes::NoteDetails;

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
    Internal(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct NoteRepositoryErrorGeneric {
    inner: Box<dyn std::error::Error + Sync + Send>,
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
            // diesel::result::Error::DatabaseError(database_error_kind, database_error_information)
            // => todo!(),
            diesel::result::Error::NotFound => NoteRepositoryError::NotFound(String::new()),
            // diesel::result::Error::QueryBuilderError(error) => todo!(),
            // diesel::result::Error::DeserializationError(error) => todo!(),
            // diesel::result::Error::SerializationError(error) => todo!(),
            // diesel::result::Error::RollbackErrorOnCommit { rollback_error, commit_error } =>
            // todo!(), diesel::result::Error::RollbackTransaction => todo!(),
            // diesel::result::Error::AlreadyInTransaction => todo!(),
            // diesel::result::Error::NotInTransaction => todo!(),
            // diesel::result::Error::BrokenTransactionManager => todo!(),
            err => NoteRepositoryError::Internal(err.into()),
        }
    }
}

impl From<deadpool_diesel::InteractError> for NoteRepositoryError {
    fn from(error: deadpool_diesel::InteractError) -> Self {
        use deadpool_diesel::InteractError;
        match error {
            InteractError::Panic(erased_type) => NoteRepositoryError::InteractDeadpool(format!(
                "interact pool: {}",
                InteractError::Panic(erased_type)
            )),
            InteractError::Aborted => {
                NoteRepositoryError::InteractDeadpool(format!("pool: {}", InteractError::Aborted))
            },
        }
    }
}

impl From<deadpool_diesel::PoolError> for NoteRepositoryError {
    fn from(error: deadpool_diesel::PoolError) -> Self {
        NoteRepositoryError::Internal(anyhow!("pool error {error}"))
    }
}

#[async_trait::async_trait]
pub trait NoteRepository: Send + Sync {
    async fn add_note(&self, note: notes::FullNote) -> Result<(), NoteRepositoryError>;

    async fn get_note_by_id(&self, note_id: &str) -> Result<FullNote, NoteRepositoryError>;

    async fn get_note_by_request_id(&self, req_id: &str) -> Result<FullNote, NoteRepositoryError>;

    async fn get_note_status_by_id(&self, note_id: &str)
    -> Result<NoteStatus, NoteRepositoryError>;

    #[allow(clippy::ptr_arg)] // TODO: fix this warning
    async fn get_note_status_by_ids(
        &self,
        note_ids: &Vec<String>,
    ) -> Result<Vec<NoteStatus>, NoteRepositoryError>;

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
    async fn get_notes_by_status_mask(
        &self,
        set_mask: Option<NoteStatus>,
        reset_mask: Option<NoteStatus>,
    ) -> Result<Vec<FullNote>, NoteRepositoryError>;

    /// Retrieves `FullNote`s from repository by status bitmask and scheduled date <= `date`
    async fn get_notes_by_status_mask_and_date(
        &self,
        set_mask: Option<NoteStatus>,
        reset_mask: Option<NoteStatus>,
        date: DateTime<Utc>,
    ) -> Result<Vec<FullNote>, NoteRepositoryError>;

    async fn mix_batch(
        &self,
        note_ids: Vec<String>,
        account_id: String,
        client: &crate::mixer::MixerClientSender,
    ) -> Result<String, NoteRepositoryError>;
}

#[async_trait::async_trait]
impl NoteRepository for DatabaseStorage {
    async fn add_note(&self, note: FullNote) -> Result<(), NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        let result = conn
            .interact(move |conn| {
                diesel::insert_into(schema::notes::table).values(&note).execute(conn)
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        if result != 1 {
            return Err(NoteRepositoryError::MoreThanOneRowAffected);
        }
        Ok(())
    }

    async fn get_note_by_id(&self, note_id: &str) -> Result<FullNote, NoteRepositoryError> {
        let find_note_id = note_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        let result = conn
            .interact(|conn| {
                schema::notes::table
                    .filter(schema::notes::note_id.eq(find_note_id))
                    .first::<FullNote>(conn)
                    .optional()
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        Ok(result.ok_or_else(|| NoteRepositoryError::NotFound(note_id.to_string()))?)
    }

    async fn get_note_by_request_id(&self, req_id: &str) -> Result<FullNote, NoteRepositoryError> {
        let find_req_id = req_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        let result = conn
            .interact(|conn| {
                schema::notes::table
                    .filter(schema::notes::request_id.eq(find_req_id))
                    .first::<FullNote>(conn)
                    .optional()
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        Ok(result.ok_or_else(|| NoteRepositoryError::NotFound(req_id.to_string()))?)
    }

    async fn get_note_status_by_id(
        &self,
        note_id: &str,
    ) -> Result<NoteStatus, NoteRepositoryError> {
        let find_note_id = note_id.to_string();

        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        let result = conn
            .interact(|conn| {
                schema::notes::table
                    .filter(schema::notes::note_id.eq(find_note_id))
                    .first::<FullNote>(conn)
                    .optional()
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        let result = result.ok_or_else(|| NoteRepositoryError::NotFound(note_id.to_string()))?;
        Ok(result.status)
    }

    /// Get notes statuses in one transaction
    /// Resulting statuses vector is guaranteed to be in the same order as input `note_ids`
    async fn get_note_status_by_ids(
        &self,
        note_ids: &Vec<String>,
    ) -> Result<Vec<NoteStatus>, NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        let note_ids = note_ids.clone();
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
        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        use diesel::result::{Error, QueryResult};

        use crate::db::schema::notes::dsl::{
            note_id as db_note_id, notes as db_notes, status as db_status,
        };
        let result = conn
            .interact(|conn| {
                conn.transaction(|conn| {
                    for (new_note_id, new_status) in note_id_statuses {
                        let rows_affected =
                            diesel::update(db_notes.filter(db_note_id.eq(new_note_id)))
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

        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        use crate::db::schema::notes::dsl::*;
        let result = conn
            .interact(move |conn| {
                diesel::update(notes.filter(note_id.eq(find_note_id)))
                    .set(status.eq(new_status))
                    .execute(conn)
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        if result != 1 {
            return Err(NoteRepositoryError::MoreThanOneRowAffected);
        }
        Ok(())
    }

    async fn get_notes_by_status_mask(
        &self,
        set_mask: Option<NoteStatus>,
        reset_mask: Option<NoteStatus>,
    ) -> Result<Vec<FullNote>, NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        use diesel::{
            dsl::sql,
            sql_types::{Bool, Integer},
        };
        let result = conn
            .interact(move |conn| {
                // any value & 0 always equal 0
                let set_mask = set_mask.unwrap_or(NoteStatus::from_bits_retain(0));
                let reset_mask = reset_mask.unwrap_or(NoteStatus::from_bits_retain(0));

                schema::notes::table
                    .filter(
                        sql::<Bool>("status & ")
                            .bind::<Integer, _>(set_mask)
                            .sql(" = ")
                            .bind::<Integer, _>(set_mask),
                    )
                    .filter(sql::<Bool>("status & ").bind::<Integer, _>(reset_mask).sql(" = 0"))
                    .load::<FullNote>(conn)
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        Ok(result)
    }

    async fn get_notes_by_status_mask_and_date(
        &self,
        set_mask: Option<NoteStatus>,
        reset_mask: Option<NoteStatus>,
        date: DateTime<Utc>,
    ) -> Result<Vec<FullNote>, NoteRepositoryError> {
        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;

        use diesel::{
            dsl::sql,
            sql_types::{Bool, Integer},
        };

        use crate::db::schema::notes::dsl as schema_dsl;
        let result = conn
            .interact(move |conn| {
                // any value & 0 always equal 0
                let set_mask = set_mask.unwrap_or(NoteStatus::from_bits_retain(0));
                let reset_mask = reset_mask.unwrap_or(NoteStatus::from_bits_retain(0));

                schema::notes::table
                    .filter(
                        sql::<Bool>("status & ")
                            .bind::<Integer, _>(set_mask)
                            .sql(" = ")
                            .bind::<Integer, _>(set_mask),
                    )
                    .filter(sql::<Bool>("status & ").bind::<Integer, _>(reset_mask).sql(" = 0"))
                    .filter(schema_dsl::scheduled_datetime.le(date.naive_utc()))
                    .load::<FullNote>(conn)
            })
            .await
            .map_err(NoteRepositoryError::from)?
            .map_err(NoteRepositoryError::from)?;

        Ok(result)
    }

    async fn mix_batch(
        &self,
        note_ids: Vec<String>,
        account_id: String,
        client: &crate::mixer::MixerClientSender,
    ) -> Result<String, NoteRepositoryError> {
        use miden_objects::{note::Note, transaction::TransactionId};

        use crate::{
            db::schema::notes::dsl::{
                note_id as db_note_id, notes as db_notes, status as db_status,
            },
            mixer::{MixClientRequest, client::MixerClientError, utils::account_from_hex},
        };

        if note_ids.len() > crate::MAX_NOTES_IN_BATCH_TRANSACTION {
            return Err(
                anyhow!("too many notes for batch transaction ({l})", l = note_ids.len()).into()
            );
        }

        let conn = self.pool.get().await.map_err(NoteRepositoryError::from)?;
        let client = client.clone();

        let result = conn
            // The closure is executed in a separate thread so that the async runtime is not blocked
            .interact(move |conn| {
                // BEGIN TRANSACTION
                conn.transaction(|conn| {
                    let mut full_notes: Vec<FullNote> = Vec::with_capacity(note_ids.len());

                    // collect notes from table
                    for note_id in note_ids.iter() {
                        // simply returns NotFound error if not exists in the table
                        // thanks to implemented From<diesel::result::Error> for NoteRepositoryError
                        let note = schema::notes::table
                            // .select(NoteDetails::as_select())
                            .filter(db_note_id.eq(note_id))
                            .first::<FullNote>(conn)?;

                        if note.account_id != account_id {
                            tracing::error!("account_id for {note_id} mismatched {account_id}!={}", note.account_id);
                            return Err(anyhow!("account {account_id} for {note_id} mismatched").into());
                        }

                        if note.status & NoteStatus::TXED == NoteStatus::TXED {
                            return Err(anyhow!("account {account_id} for {note_id} status already txed").into());
                        }

                        full_notes.push(note);
                    }

                    tracing::info!("Reading account {account_id}");
                    let faucet_id = account_from_hex(&account_id)?;

                    // deserialize notes from strings
                    tracing::info!("Reading notes for {account_id}");
                    let miden_notes: Vec<Note> = full_notes
                        .iter()
                        .map(Note::try_from)
                        .collect::<Result<Vec<_>, _>>()?;

                    // send batch request
                    let (request, response) = tokio::sync::oneshot::channel::<Result<TransactionId, MixerClientError>>();
                    client
                        .blocking_send(MixClientRequest::MixBatch {
                            notes: miden_notes,
                            account_id: faucet_id,
                            response_sink: request,
                        })
                        .map_err(|e| NoteRepositoryError::Internal(anyhow!("client send error {e}")))?;

                    // await for result of mixing (transaction id)
                    let tx_id = response.blocking_recv()
                        .with_context(|| format!("response mix batch error for {account_id}"))?
                        .with_context(|| format!("internal mix batch error for {account_id}"))?;

                    // update notes' statuses in the table
                    for note_id in note_ids.iter() {
                        let status = schema::notes::table
                            .select(NoteDetails::as_select())
                            .filter(db_note_id.eq(note_id))
                            .first::<NoteDetails>(conn)?
                            .status;

                        let new_status = status | NoteStatus::TXED;

                        if diesel::update(db_notes.filter(db_note_id.eq(note_id)))
                            .set(db_status.eq(new_status))
                            .execute(conn)? != 1 {
                                return Err(NoteRepositoryError::MoreThanOneRowAffected);
                            }
                    }

                    Ok(tx_id.to_string()) // TODO: how does this error coercion works? 0_o
                })
                // END TRANSACTION
            })
            .await??;

        Ok(result)
    }
}

impl TryFrom<&FullNote> for miden_objects::note::Note {
    type Error = anyhow::Error;

    fn try_from(full_note: &FullNote) -> Result<Self, Self::Error> {
        use miden_objects::{note::Note, utils::Deserializable};

        let FullNote { note_id, note, .. } = full_note;

        let note_bytes = hex::decode(note)
            .with_context(|| format!("decoding from hex string note {note_id}"))?;
        let note = Note::read_from_bytes(note_bytes.as_slice())
            .with_context(|| format!("reading note from bytes for {note_id}"))?;
        Ok(note)
    }
}

// TODO: test repository trait methods with MockNotes (todo)
// #[cfg(test)]
// mod test {
//     use crate::db::test::Fixture;

//     #[tokio::test]
//     async fn test_multi_status_fetch() {
//         let fixture = Fixture::prepare().await;
//         let db = fixture.db();

//         let

//         db.get_notes_by_status(req_status)

//     }
// }

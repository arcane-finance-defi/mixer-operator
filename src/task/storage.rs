use chrono::{ Utc, DateTime };
use miden_objects::note::NoteId;
use crate::db::models::{ 
    notes::{FullNote, NoteStatus}, NoteRepository
};

#[tracing::instrument(skip(storage))]
pub(super) async fn set_note_txed(storage: &dyn NoteRepository, note_id: NoteId) -> anyhow::Result<()> {
    match storage.update_note_status_by_id(&note_id.to_string(), NoteStatus::TXED).await {
        Ok(_) => Ok(()),
        Err(err) => anyhow::bail!("update notes status error {err:#?}"),
    }
}

#[tracing::instrument(skip(storage))]
pub(super) async fn poll_for_ready_notes(storage: &dyn NoteRepository, date: DateTime<Utc>) -> anyhow::Result<Vec<FullNote>> {
    let status = NoteStatus::ACCEPTED & !NoteStatus::TXED;
    
    let notes = match storage.get_notes_by_status_and_date(status, date).await {
        Ok(notes) => notes,
        Err(err) => anyhow::bail!("reading notes storage error {err:#?}"),
    };

    Ok(notes)
}
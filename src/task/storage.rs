use anyhow::bail;
use chrono::{ Utc, DateTime };
use miden_objects::note::NoteId;
use crate::db::models::{ 
    notes::{FullNote, NoteStatus}, NoteRepository
};

#[tracing::instrument(skip(storage))]
pub(super) async fn set_note_txed(storage: &dyn NoteRepository, note_id: NoteId) -> anyhow::Result<()> {
    match storage.update_note_status_by_id(&note_id.to_string(), NoteStatus::TXED).await {
        Ok(_) => Ok(()),
        Err(err) => bail!("update notes status error {err:#?}"),
    }
}

#[tracing::instrument(skip(storage))]
pub(super) async fn poll_for_ready_notes(storage: &dyn NoteRepository, date: DateTime<Utc>) -> anyhow::Result<Vec<FullNote>> {
    let status = NoteStatus::ACCEPTED & !NoteStatus::TXED & !NoteStatus::PROCESSING;
    
    let notes = match storage.get_notes_by_status_and_date(status, date).await {
        Ok(notes) => notes,
        Err(err) => bail!("reading notes storage error {err:#?}"),
    };

    Ok(notes)
}

// TODO: должен устанавливать статус одной транзакцией сразу на все ноты
pub(super) async fn set_note_processing(storage: &dyn NoteRepository, note_ids: &[&str]) -> anyhow::Result<()> {
    for note_id in note_ids {
        let status = match storage.get_note_status_by_id(note_id).await {
            Ok(status) => status,
            Err(err) => bail!("set_note_processing fetch status error {err:#?}"),
        };

        if status & NoteStatus::PROCESSING == NoteStatus::PROCESSING {
            bail!("note {note_id} already in processing");
        }

        let new_status = status | NoteStatus::PROCESSING;
        if let Err(err) = storage.update_note_status_by_id(note_id, new_status).await {
            bail!("unable to update processing status for {note_id} with error {err:#?}");
        }
    }
    Ok(())
}
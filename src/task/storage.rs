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

pub(super) async fn set_note_processing(storage: &dyn NoteRepository, note_ids: &[&str]) -> anyhow::Result<()> {
    let note_ids: Vec<String> = note_ids.iter().map(|n| n.to_string()).collect(); 
    let statuses = match storage.get_note_status_by_ids(&note_ids).await {
        Ok(status) => status,
        Err(err) => bail!("set_note_processing fetch status error {err:#?}"),
    };

    let mut new_statuses: Vec<_> = Vec::with_capacity(statuses.len());
    for (idx, status) in statuses.iter().enumerate() {
        if *status & NoteStatus::PROCESSING != NoteStatus::PROCESSING {
            let new_status = *status | NoteStatus::PROCESSING;
            new_statuses.push((note_ids[idx].to_string(), new_status));
        } else {
            bail!("note {} already in processing", note_ids[idx]);
        }
    }

    if let Err(err) = storage.update_note_status_by_ids(new_statuses).await {
        bail!("unable to update processing status with error {err:#?}");
    }
    Ok(())
}
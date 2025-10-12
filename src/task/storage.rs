use anyhow::bail;
use chrono::{DateTime, Utc};
use miden_objects::note::NoteId;

use crate::db::models::{
    NoteRepository,
    notes::{FullNote, NoteStatus},
};

// ! with incorrect usage will clear any other flags
#[tracing::instrument(skip(storage))]
pub(super) async fn set_note_txed(
    storage: &dyn NoteRepository,
    note_id: NoteId,
) -> anyhow::Result<()> {
    match storage.update_note_status_by_id(&note_id.to_string(), NoteStatus::TXED).await {
        Ok(_) => Ok(()),
        Err(err) => bail!("update notes status error {err:#?}"),
    }
}

#[allow(dead_code)]
#[tracing::instrument(skip(storage, note_ids))]
pub(super) async fn set_notes_txed(
    storage: &dyn NoteRepository,
    note_ids: &Vec<&str>,
) -> anyhow::Result<()> {
    let id_and_statuses: Vec<(_, _)> =
        note_ids.iter().map(|id| (id.to_string(), NoteStatus::TXED)).collect();
    if let Err(err) = storage.update_note_status_by_ids(id_and_statuses).await {
        bail!("unable to update txed status with error {err:#?}");
    }
    Ok(())
}

#[tracing::instrument(skip(storage))]
pub(super) async fn poll_for_ready_notes(
    storage: &dyn NoteRepository,
    date: DateTime<Utc>,
) -> anyhow::Result<Vec<FullNote>> {
    let status = NoteStatus::ACCEPTED & !NoteStatus::TXED & !NoteStatus::PROCESSING;

    let notes = match storage.get_notes_by_status_and_date(status, date).await {
        Ok(notes) => notes,
        Err(err) => bail!("reading notes storage error {err:#?}"),
    };

    Ok(notes)
}

#[allow(dead_code)]
pub(super) async fn set_note_processing(
    storage: &dyn NoteRepository,
    note_ids: &[&str],
    processing: bool,
) -> anyhow::Result<()> {
    let note_ids: Vec<String> = note_ids.iter().map(|n| n.to_string()).collect();
    let statuses = match storage.get_note_status_by_ids(&note_ids).await {
        Ok(status) => status,
        Err(err) => bail!("set_note_processing fetch status error {err:#?}"),
    };

    let mut new_statuses: Vec<_> = Vec::with_capacity(statuses.len());
    for (idx, status) in statuses.iter().enumerate() {
        if processing {
            // set PROCESSING bit
            if *status & NoteStatus::PROCESSING != NoteStatus::PROCESSING {
                let new_status = *status | NoteStatus::PROCESSING;
                new_statuses.push((note_ids[idx].to_string(), new_status));
            } else {
                bail!("note #{idx} {} already IN processing", note_ids[idx]);
            }
        } else {
            // reset PROCESSING bit
            if *status & NoteStatus::PROCESSING != NoteStatus::PROCESSING {
                tracing::error!("note #{idx} {} already NOT IN processing", note_ids[idx]);
                tracing::error!("reset bitflag anyway");
            }
            let new_status = *status & !NoteStatus::PROCESSING;
            new_statuses.push((note_ids[idx].to_string(), new_status));
        }
    }

    if let Err(err) = storage.update_note_status_by_ids(new_statuses).await {
        bail!("unable to update processing status with error {err:#?}");
    }
    Ok(())
}

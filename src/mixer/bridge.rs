/// ! 
/// ! Module mainly re-exporting bridge functions and utils
/// ! 

use thiserror::Error;

use miden_objects::{
    NoteError, Word, ZERO,
    asset::Asset,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteFile, NoteId, NoteInputs, NoteMetadata,
        NoteRecipient, NoteType,
    },
};

/// 
/// Bridge re-export
/// 
pub(super) use miden_bridge::{
    accounts::token_wrapper::bridge_note_tag,
    notes::bridge::{bridge, croschain},
};

///
/// Utilities
/// 
#[derive(Error, Debug)]
pub enum PublicNoteConstructorError {
    #[error("Fungible asset in the crosschain note is not found")]
    FungibleAssetNotFound(),
    #[error(transparent)]
    NoteCreationError(#[from] NoteError),
    #[error("Malformed serial number")]
    MalformedSerialNumber(),
}


pub fn get_public_bridge_output_note(input_note: &Note) -> Result<Note, PublicNoteConstructorError> {
    let crosschain_asset = input_note
        .assets()
        .iter()
        .last()
        .ok_or(PublicNoteConstructorError::FungibleAssetNotFound())?;

    let crosschain_asset = match crosschain_asset {
        Asset::Fungible(asset) => Ok(asset),
        _ => Err(PublicNoteConstructorError::FungibleAssetNotFound()),
    }?;

    let script = bridge();
    let assets = NoteAssets::default();
    let metadata = NoteMetadata::new(
        crosschain_asset.faucet_id(),
        NoteType::Public,
        bridge_note_tag(),
        NoteExecutionHint::Always,
        ZERO,
    )?;

    let serial_num = Word::try_from(&input_note.inputs().values()[..4])
        .map_err(|_| PublicNoteConstructorError::MalformedSerialNumber())?;

    let inputs = NoteInputs::new(
        [
            Word::from(Asset::Fungible(*crosschain_asset)).to_vec(),
            input_note.inputs().values()[4..].to_vec(),
        ]
        .concat(),
    )?;

    let recipient = NoteRecipient::new(serial_num, script, inputs);

    Ok(Note::new(assets, metadata, recipient))
}
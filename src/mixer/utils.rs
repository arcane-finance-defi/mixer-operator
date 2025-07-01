use anyhow::Context;
use miden_objects::utils::Deserializable;
use miden_objects::note::{NoteId, NoteFile};

pub fn extract_note_id(note_file: NoteFile) -> NoteId {
    match &note_file {
        NoteFile::NoteId(id) => *id,
        NoteFile::NoteDetails { details, .. } => details.id(),
        NoteFile::NoteWithProof(note, _) => note.id(),
    }
}

pub fn is_note_with_proof(note_file: NoteFile) -> bool {
    matches!(note_file, NoteFile::NoteWithProof(..))
}

// can't use TryFrom trait without NewType due to Rust orphan's rules
pub fn from_hex_string(hexstr: &str) -> anyhow::Result<NoteFile> {
    let note_bytes = hex::decode(&hexstr).context("decoding from hex &str")?;
    let note_file = NoteFile::read_from_bytes(note_bytes.as_slice()).context("reading note file from bytes")?;
    Ok(note_file)
}
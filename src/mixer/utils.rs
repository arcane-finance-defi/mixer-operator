use anyhow::Context;
use miden_objects::Word;
use miden_objects::note::{NoteFile, NoteId};
use miden_objects::utils::{Deserializable, Serializable, ToHex};

pub fn extract_note_id(note_file: &NoteFile) -> NoteId {
    match note_file {
        NoteFile::NoteId(id) => *id,
        NoteFile::NoteDetails { details, .. } => details.id(),
        NoteFile::NoteWithProof(note, _) => note.id(),
    }
}

pub fn is_note_with_proof(note_file: &NoteFile) -> bool {
    matches!(note_file, NoteFile::NoteWithProof(..))
}

// can't use TryFrom trait without NewType due to Rust orphan's rules
pub fn from_hex_string(hexstr: &str) -> anyhow::Result<NoteFile> {
    let note_bytes = hex::decode(hexstr).context("decoding from hex &str")?;
    let note_file =
        NoteFile::read_from_bytes(note_bytes.as_slice()).context("reading note file from bytes")?;
    Ok(note_file)
}

pub fn to_hex_string(note_file: NoteFile) -> String {
    note_file.to_bytes().to_hex()
}

pub fn word_from_hex(hexstr: &str) -> anyhow::Result<Word> {
    let bytes = hex::decode(&hexstr).context("decoding from hex str")?;
    let word = Word::read_from_bytes(&bytes).context("reading word from bytes")?;

    Ok(word)
}

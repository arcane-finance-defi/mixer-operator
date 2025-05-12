#![allow(clippy::items_after_statements)]

use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
    vec::Vec,
};
use std::rc::Rc;

use miden_objects::{
    Digest,
    account::AccountId,
    block::BlockNumber,
    crypto::utils::{Deserializable, Serializable},
    transaction::{
        ExecutedTransaction, OutputNotes, ToInputNoteCommitments, TransactionId, TransactionScript,
    },
};
use rusqlite::{Connection, Transaction, params, types::Value};
use crate::transaction::{TransactionRecord, TransactionStatus, TransactionUpdate};
use super::{store::{BlockStore, StoreError}, TransactionFilter};

pub(crate) const INSERT_TRANSACTION_QUERY: &str = "INSERT INTO transactions (id, account_id, init_account_state, final_account_state, \
    input_notes, output_notes, script_root, block_num, commit_height, discarded) \
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

pub(crate) const INSERT_TRANSACTION_SCRIPT_QUERY: &str = "INSERT OR IGNORE INTO transaction_scripts (script_root, script) \
    VALUES (?, ?)";

// TRANSACTIONS FILTERS
// ================================================================================================

impl TransactionFilter {
    /// Returns a [String] containing the query for this Filter.
    pub fn to_query(&self) -> String {
        const QUERY: &str = "SELECT tx.id, tx.account_id, tx.init_account_state, tx.final_account_state, \
            tx.input_notes, tx.output_notes, tx.script_root, script.script, tx.block_num, tx.commit_height, \
            tx.discarded
            FROM transactions AS tx LEFT JOIN transaction_scripts AS script ON tx.script_root = script.script_root";
        match self {
            TransactionFilter::All => QUERY.to_string(),
            TransactionFilter::Uncomitted => format!("{QUERY} WHERE tx.commit_height IS NULL"),
            TransactionFilter::Ids(_) => {
                // Use SQLite's array parameter binding
                format!("{QUERY} WHERE tx.id IN rarray(?)")
            },
            TransactionFilter::ExpiredBefore(block_num) => {
                format!(
                    "{QUERY} WHERE tx.block_num < {} AND tx.discarded = false AND tx.commit_height IS NULL",
                    block_num.as_u32()
                )
            },
        }
    }
}

// TRANSACTIONS
// ================================================================================================

type SerializedTransactionData = (
    String,
    String,
    String,
    String,
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    u32,
    Option<u32>,
    bool,
);

impl BlockStore {
    /// Retrieves tracked transactions, filtered by [`TransactionFilter`].
    pub fn get_transactions(
        conn: &mut Connection,
        filter: &TransactionFilter,
    ) -> Result<Vec<TransactionRecord>, StoreError> {
        match filter {
            TransactionFilter::Ids(ids) => {
                // Convert transaction IDs to strings for the array parameter
                let id_strings =
                    ids.iter().map(|id| Value::Text(id.to_string())).collect::<Vec<_>>();

                // Create a prepared statement and bind the array parameter
                conn.prepare(&filter.to_query())?
                    .query_map(params![Rc::new(id_strings)], parse_transaction_columns)?
                    .map(|result| Ok(result?).and_then(parse_transaction))
                    .collect::<Result<Vec<TransactionRecord>, _>>()
            },
            _ => {
                // For other filters, no parameters are needed
                conn.prepare(&filter.to_query())?
                    .query_map([], parse_transaction_columns)?
                    .map(|result| Ok(result?).and_then(parse_transaction))
                    .collect::<Result<Vec<TransactionRecord>, _>>()
            },
        }
    }

    /// Set the provided transactions as committed.
    ///
    /// # Errors
    ///
    /// This function can return an error if any of the updates to the transactions within the
    /// database transaction fail.
    pub(crate) fn mark_transactions_as_committed(
        tx: &Transaction<'_>,
        transactions_to_commit: &[TransactionUpdate],
    ) -> Result<usize, StoreError> {
        let mut rows = 0;
        for transaction_update in transactions_to_commit {
            const QUERY: &str = "UPDATE transactions set commit_height=? where id=?";
            rows += tx.execute(
                QUERY,
                params![
                    Some(transaction_update.block_num),
                    transaction_update.transaction_id.to_string()
                ],
            )?;
        }
        info!("Marked {} transactions as committed", rows);

        Ok(rows)
    }

    /// Set the provided transactions as committed.
    ///
    /// # Errors
    ///
    /// This function can return an error if any of the updates to the transactions within the
    /// database transaction fail.
    pub(crate) fn mark_transactions_as_discarded(
        tx: &Transaction<'_>,
        transactions_to_discard: &[TransactionId],
    ) -> Result<usize, StoreError> {
        let mut rows = 0;
        for transaction_id in transactions_to_discard {
            const QUERY: &str = "UPDATE transactions set discarded=true where id=?";
            rows += tx.execute(QUERY, params![transaction_id.to_string()])?;
        }

        Ok(rows)
    }
}

fn parse_transaction_columns(
    row: &rusqlite::Row<'_>,
) -> Result<SerializedTransactionData, rusqlite::Error> {
    let id: String = row.get(0)?;
    let account_id: String = row.get(1)?;
    let init_account_state: String = row.get(2)?;
    let final_account_state: String = row.get(3)?;
    let input_notes: Vec<u8> = row.get(4)?;
    let output_notes: Vec<u8> = row.get(5)?;
    let script_root: Option<Vec<u8>> = row.get(6)?;
    let tx_script: Option<Vec<u8>> = row.get(7)?;
    let block_num: u32 = row.get(8)?;
    let commit_height: Option<u32> = row.get(9)?;
    let discarded: bool = row.get(10)?;

    Ok((
        id,
        account_id,
        init_account_state,
        final_account_state,
        input_notes,
        output_notes,
        script_root,
        tx_script,
        block_num,
        commit_height,
        discarded,
    ))
}

/// Parse a transaction from the provided parts.
fn parse_transaction(
    serialized_transaction: SerializedTransactionData,
) -> Result<TransactionRecord, StoreError> {
    let (
        id,
        account_id,
        init_account_state,
        final_account_state,
        input_notes,
        output_notes,
        _script_root,
        tx_script,
        block_num,
        commit_height,
        discarded,
    ) = serialized_transaction;
    let account_id = AccountId::from_hex(&account_id)?;
    let id: Digest = id.try_into()?;
    let init_account_state: Digest = init_account_state.try_into()?;
    let final_account_state: Digest = final_account_state.try_into()?;

    let input_note_nullifiers: Vec<Digest> = Vec::<Digest>::read_from_bytes(&input_notes)
        .map_err(StoreError::DataDeserializationError)?;

    let output_notes = OutputNotes::read_from_bytes(&output_notes)?;

    let transaction_script: Option<TransactionScript> = tx_script
        .map(|script| TransactionScript::read_from_bytes(&script))
        .transpose()?;

    let transaction_status = if discarded {
        TransactionStatus::Discarded
    } else {
        let commit_height = commit_height.map(BlockNumber::from);
        commit_height.map_or(TransactionStatus::Pending, TransactionStatus::Committed)
    };

    Ok(TransactionRecord {
        id: id.into(),
        account_id,
        init_account_state,
        final_account_state,
        input_note_nullifiers,
        output_notes,
        transaction_script,
        block_num: block_num.into(),
        transaction_status,
    })
}

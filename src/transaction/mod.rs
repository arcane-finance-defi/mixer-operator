use core::fmt;
use miden_client::crypto::Digest;
use miden_objects::account::{Account, AccountId};
use miden_objects::block::BlockNumber;
use miden_objects::note::Note;
use miden_objects::transaction::{ExecutedTransaction, OutputNotes, TransactionId, TransactionScript};
use thiserror::Error;

mod request;

use request::TransactionRequest;



/// Represents the status of a transaction.
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    /// Transaction has been submitted but not yet committed.
    Pending,
    /// Transaction has been committed and included at the specified block number.
    Committed(BlockNumber),
    /// Transaction has been discarded and isn't included in the node.
    Discarded,
}

impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionStatus::Pending => write!(f, "Pending"),
            TransactionStatus::Committed(block_number) => {
                write!(f, "Committed (Block: {block_number})")
            },
            TransactionStatus::Discarded => write!(f, "Discarded"),
        }
    }
}


/// Describes a transaction that has been executed and is being tracked on the Client.
///
/// Currently, the `commit_height` (and `committed` status) is set based on the height
/// at which the transaction's output notes are committed.
#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub id: TransactionId,
    pub account_id: AccountId,
    pub init_account_state: Digest,
    pub final_account_state: Digest,
    pub input_note_nullifiers: Vec<Digest>,
    pub output_notes: OutputNotes,
    pub transaction_script: Option<TransactionScript>,
    pub block_num: BlockNumber,
    pub transaction_status: TransactionStatus,
}

impl TransactionRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: TransactionId,
        account_id: AccountId,
        init_account_state: Digest,
        final_account_state: Digest,
        input_note_nullifiers: Vec<Digest>,
        output_notes: OutputNotes,
        transaction_script: Option<TransactionScript>,
        block_num: BlockNumber,
        transaction_status: TransactionStatus,
    ) -> TransactionRecord {
        TransactionRecord {
            id,
            account_id,
            init_account_state,
            final_account_state,
            input_note_nullifiers,
            output_notes,
            transaction_script,
            block_num,
            transaction_status,
        }
    }
}

/// Represents a transaction that was included in the node at a certain block.
#[derive(PartialEq, Eq)]
pub struct TransactionUpdate {
    /// The transaction identifier.
    pub transaction_id: TransactionId,
    /// The number of the block in which the transaction was included.
    pub block_num: u32,
    /// The account that the transcation was executed against.
    pub account_id: AccountId,
}

#[derive(Error)]
pub enum TransactionError {

}

pub async fn new_transaction(
    account: Account,
    transaction_request: TransactionRequest,
) -> Result<ExecutedTransaction, TransactionError> {

    let note_ids = transaction_request.get_input_note_ids();

    let output_notes: Vec<Note> =
        transaction_request.expected_output_notes().cloned().collect();

    let tx_script = transaction_request.build_transaction_script(
        &self.get_account_interface(account_id).await?,
        self.in_debug_mode,
    )?;

    let foreign_accounts = transaction_request.foreign_accounts().clone();
    let mut tx_args = transaction_request.into_transaction_args(tx_script);

    // Inject state and code of foreign accounts
    let fpi_block_num =
        self.inject_foreign_account_inputs(foreign_accounts, &mut tx_args).await?;

    let block_num = if let Some(block_num) = fpi_block_num {
        block_num
    } else {
        self.store.get_sync_height().await?
    };

    // Execute the transaction and get the witness
    let executed_transaction = self
        .tx_executor
        .execute_transaction(account_id, block_num, &note_ids, tx_args)
        .await?;

    // Check that the expected output notes matches the transaction outcome.
    // We compare authentication commitments where possible since that involves note IDs +
    // metadata (as opposed to just note ID which remains the same regardless of
    // metadata) We also do the check for partial output notes

    let tx_note_auth_commitments: BTreeSet<Digest> =
        notes_from_output(executed_transaction.output_notes())
            .map(Note::commitment)
            .collect();

    let missing_note_ids: Vec<NoteId> = output_notes
        .iter()
        .filter_map(|n| (!tx_note_auth_commitments.contains(&n.commitment())).then_some(n.id()))
        .collect();

    if !missing_note_ids.is_empty() {
        return Err(ClientError::MissingOutputNotes(missing_note_ids));
    }

    let screener = NoteScreener::new(self.store.clone());

    TransactionResult::new(
        executed_transaction,
        screener,
        future_notes,
        self.get_sync_height().await?,
        self.store.get_current_timestamp(),
    )
        .await
}
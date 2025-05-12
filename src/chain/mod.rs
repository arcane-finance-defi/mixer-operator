use miden_client::crypto::InOrderIndex;
use miden_objects::block::BlockNumber;
use miden_objects::transaction::TransactionId;

mod store;
mod sync;
mod chain_data;
mod errors;
mod transaction;
mod account;
mod sqlutils;

pub enum ChainMmrNodeFilter {
    /// Return all nodes.
    All,
    /// Filter by the specified in-order indices.
    List(Vec<InOrderIndex>),
}


/// Filters for narrowing the set of transactions returned by the client's store.
#[derive(Debug, Clone)]
pub enum TransactionFilter {
    /// Return all transactions.
    All,
    /// Filter by transactions that haven't yet been committed to the blockchain as per the last
    /// sync.
    Uncomitted,
    /// Return a list of the transaction that matches the provided [`TransactionId`]s.
    Ids(Vec<TransactionId>),
    /// Return a list of the expired transactions that were executed before the provided
    /// [`BlockNumber`]. Transactions created after the provided block number are not
    /// considered.
    ///
    /// A transaction is considered expired if is uncommitted and the transaction's block number
    /// is less than the provided block number.
    ExpiredBefore(BlockNumber),
}
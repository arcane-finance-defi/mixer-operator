#![allow(clippy::items_after_statements)]


use miden_objects::block::BlockNumber;
use rusqlite::{Connection, params};
use crate::chain::account::update_account;
use crate::sync::StateSyncUpdate;
use super::store::{BlockStore, StoreError};

impl BlockStore {
    pub(super) fn get_sync_height(conn: &mut Connection) -> Result<BlockNumber, StoreError> {
        const QUERY: &str = "SELECT block_num FROM state_sync";

        conn.prepare(QUERY)?
            .query_map([], |row| row.get(0))
            .expect("no binding parameters used in query")
            .map(|result| {
                Ok(result?).map(|v: i64| {
                    BlockNumber::from(u32::try_from(v).expect("block number is always positive"))
                })
            })
            .next()
            .expect("state sync block number exists")
    }

    pub(super) fn apply_state_sync(
        conn: &mut Connection,
        state_sync_update: StateSyncUpdate,
    ) -> Result<(), StoreError> {
        let StateSyncUpdate {
            block_header,
            block_has_relevant_notes,
            new_mmr_peaks,
            new_authentication_nodes,
            note_updates,
            transaction_updates,
            account_updates,
            tags_to_remove,
        } = state_sync_update;

        let tx = conn.transaction()?;

        // Update state sync block number
        const BLOCK_NUMBER_QUERY: &str = "UPDATE state_sync SET block_num = ?";
        tx.execute(BLOCK_NUMBER_QUERY, params![i64::from(block_header.block_num().as_u32())])?;

        Self::insert_block_header_tx(&tx, &block_header, &new_mmr_peaks, block_has_relevant_notes)?;

        // Insert new authentication nodes (inner nodes of the PartialMmr)
        Self::insert_chain_mmr_nodes_tx(&tx, &new_authentication_nodes)?;

        // Mark transactions as committed
        Self::mark_transactions_as_committed(&tx, transaction_updates.committed_transactions())?;

        // Update public accounts on the db that have been updated onchain
        for account in account_updates.updated_public_accounts() {
            update_account(&tx, account)?;
        }

        // Commit the updates
        tx.commit()?;

        Ok(())
    }
}

use std::path::PathBuf;
use deadpool_sqlite::{Config, Hook, HookError, Pool, PoolError, Runtime};
use miden_client::utils::DeserializationError;
use miden_objects::{AccountError, AccountIdError, AssetVaultError};
use miden_objects::account::AccountId;
use miden_objects::crypto::merkle::MmrError;
use miden_objects::utils::HexParseError;
use rusqlite::vtab::array;
use thiserror::Error;

pub struct BlockStore {
    pub(crate) pool: Pool,
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("failed to parse data retrieved from the database: {0}")]
    ParsingError(String),
    #[error("failed to retrieve data from the database: {0}")]
    QueryError(String),
    #[error("error constructing mmr")]
    MmrError(#[from] MmrError),
    #[error("error deserializing data from the store")]
    DataDeserializationError(#[from] DeserializationError),
    #[error("error parsing hex")]
    HexParseError(#[from] HexParseError),
    #[error("account id error")]
    AccountIdError(#[from] AccountIdError),
    #[error("account error")]
    AccountError(#[from] AccountError),
    #[error("account data wasn't found for account id {0}")]
    AccountDataNotFound(AccountId),
    #[error("asset vault error")]
    AssetVaultError(#[from] AssetVaultError),
    #[error(transparent)]
    PoolError(#[from] PoolError),
}

impl BlockStore {

    pub async fn new(database_filepath: PathBuf) -> Result<Self, StoreError> {
        let database_exists = database_filepath.exists();

        let connection_cfg = Config::new(database_filepath);
        let pool = connection_cfg
            .builder(Runtime::Tokio1)
            .map_err(|err| StoreError::DatabaseError(err.to_string()))?
            .post_create(Hook::async_fn(move |conn, _| {
                Box::pin(async move {
                    // Feature used to support `IN` and `NOT IN` queries. We need to load this
                    // module for every connection we create to the DB to
                    // support the queries we want to run
                    conn.interact(|conn| array::load_module(conn))
                        .await
                        .map_err(|_| HookError::message("Loading rarray module failed"))?
                        .map_err(|err| HookError::message(err.to_string()))?;

                    Ok(())
                })
            }))
            .build()
            .map_err(|err| StoreError::DatabaseError(err.to_string()))?;

        if !database_exists {
            pool.get()
                .await
                .map_err(|err| StoreError::DatabaseError(err.to_string()))?
                .interact(|conn| conn.execute_batch(include_str!("store.sql")))
                .await
                .map_err(|err| StoreError::DatabaseError(err.to_string()))?
                .map_err(|err| StoreError::DatabaseError(err.to_string()))?;
        }

        Ok(Self { pool })
    }

}
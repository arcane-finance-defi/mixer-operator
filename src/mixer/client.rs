use std::{path::PathBuf, sync::Arc};

use glob::glob;
use miden_client::{
    Client as MidenClient, ClientError as MidenClientError, ExecutionOptions,
    account::{AccountFile, AccountId},
    auth::BasicAuthenticator,
    crypto::RpoRandomCoin,
    note::{Note, NoteFile, NoteId},
    rpc::{Endpoint, NodeRpcClient, RpcError, TonicRpcClient},
    store::{Store, sqlite_store::SqliteStore},
    transaction::{NoteArgs, TransactionId, TransactionRequestBuilder, TransactionRequestError},
    utils::{Deserializable, DeserializationError},
};
use miden_objects::{AccountIdError, Felt, MAX_TX_EXECUTION_CYCLES, MIN_TX_EXECUTION_CYCLES, Word};
use rand::{Rng, rng, rngs::StdRng};
use thiserror::Error;
use tokio::fs::read;
use tracing::info;

use super::bridge::{PublicNoteConstructorError, croschain, get_public_bridge_output_note};
use crate::MAX_NOTES_IN_BATCH_TRANSACTION;

const DEFAULT_STORAGE_FILE: &str = "store.db";

// NB: waiting for Send trait support (https://github.com/0xMiden/miden-client/pull/1015) o
// or could be reimplemented using https://docs.rs/tokio/latest/tokio/task/fn.spawn_local.html
pub struct MixerClient {
    client: MidenClient<BasicAuthenticator<StdRng>>,
    rpc: Arc<dyn NodeRpcClient>,
}

#[derive(Error, Debug)]
pub enum MixerClientError {
    #[error(transparent)]
    InternalClientError(#[from] MidenClientError),
    #[error("Endpoint string parse error: {0}")]
    MalformedEndpointUrlError(String),
    #[error("Invalid note type")]
    InvalidNoteTypeError(),
    #[error("Not manageable account {0}")]
    NotManageableAccountError(String),
    #[error("Too many notes for signle transaction")]
    TransactionNotesLimit(),
    #[error(transparent)]
    TransactionRequestError(#[from] TransactionRequestError),
    #[error("Wrong input note script root")]
    WrongNoteScriptRootError(),
    #[error(transparent)]
    EventNoteConstructorError(#[from] PublicNoteConstructorError),
    #[error(transparent)]
    InternalIoError(#[from] std::io::Error),
    #[error(transparent)]
    InternalDeserializationError(#[from] DeserializationError),
    #[error(transparent)]
    AccountIdParsingError(#[from] AccountIdError),
    #[error(transparent)]
    RpcError(#[from] RpcError),
}

impl MixerClient {
    pub async fn new(
        rpc_endpoint: &str,
        rpc_timeout_ms: u64,
        store_filename: Option<PathBuf>,
        debug: bool,
    ) -> Result<Self, MixerClientError> {
        let store = SqliteStore::new(
            store_filename.unwrap_or(PathBuf::from(DEFAULT_STORAGE_FILE.to_string())),
        )
        .await
        .map_err(MidenClientError::StoreError)?;

        let store = Arc::new(store);

        let mut rng = rng();
        let coin_seed: [u64; 4] = rng.random();

        let rng = RpoRandomCoin::new(Word::from(coin_seed.map(Felt::new)));

        let rpc = Arc::new(TonicRpcClient::new(
            &Endpoint::try_from(rpc_endpoint)
                .map_err(MixerClientError::MalformedEndpointUrlError)?,
            rpc_timeout_ms,
        ));

        let client = MidenClient::new(
            rpc.clone(),
            Box::new(rng),
            store.clone() as Arc<dyn Store>,
            Some(Arc::new(BasicAuthenticator::<StdRng>::new(&[]))),
            ExecutionOptions::new(
                Some(MAX_TX_EXECUTION_CYCLES),
                MIN_TX_EXECUTION_CYCLES,
                debug,
                debug,
            )
            .unwrap(),
            None,
            None,
        )
        .await?;

        Ok(Self { client, rpc })
    }

    pub async fn initialize(
        &mut self,
        supported_accounts_dir: PathBuf,
        public_accounts_to_import: Vec<String>,
    ) -> Result<(), MixerClientError> {
        info!("Mixer operator initialization start");
        let mut supported_accounts_dir = supported_accounts_dir.to_str().unwrap().to_string();
        supported_accounts_dir.push_str("/*.mac");

        self.client.sync_state().await?;
        info!("Mixer state synced");

        for path in glob(supported_accounts_dir.as_str()).unwrap().filter_map(Result::ok) {
            let account_bytes = read(path).await?;
            let account_file = AccountFile::read_from_bytes(account_bytes.as_slice())?;
            let account_id = account_file.account.id();
            let account_id_hex = account_id.to_hex();
            info!("Importing the private account with id {account_id_hex}");

            if self.client.try_get_account_header(account_id).await.is_err() {
                self.client.add_account(&account_file.account, None, false).await?;
            }
            info!("Private account imported")
        }

        for public_account_id in public_accounts_to_import {
            info!("Importing the public account with id {public_account_id}");
            let public_account_id = AccountId::from_hex(public_account_id.as_str())?;

            if self.client.try_get_account_header(public_account_id).await.is_err() {
                self.client.import_account_by_id(public_account_id).await?;
            }
            info!("Public account imported")
        }

        Ok(())
    }

    async fn cleanup(&self) -> Result<(), MixerClientError> {
        info!("Clean-up");
        // self.store.remove_notes().await?;

        Ok(())
    }

    // #[tracing::instrument(skip_all)]
    // TODO: should be deprecated in favour of mix_batch unified implementation
    // TODO: or keep API compatibility and use mix_batch implementation underneath
    pub async fn mix(
        &mut self,
        note: Note,
        account_id: AccountId,
    ) -> Result<String, MixerClientError> {
        if note.recipient().script().root() == croschain().root() {
            Ok(())
        } else {
            Err(MixerClientError::WrongNoteScriptRootError())
        }?;

        // reconstruct expected note from the bridge
        let expected_bridge_note = get_public_bridge_output_note(&note)?;

        // sync state with blockchain
        self.client.sync_state().await?;

        // obtain account to consume to
        let account = self.client.try_get_account(account_id).await;

        // TODO: semantically wrong errors cast
        if let Err(MidenClientError::AccountDataNotFound(_)) = account {
            Err(MixerClientError::NotManageableAccountError(account_id.to_hex()))
        } else {
            Ok(())
        }?;

        // sync state
        self.client.sync_state().await?;

        info!("Build transaction request");
        let tx_request = TransactionRequestBuilder::new()
            .expected_output_recipients(vec![expected_bridge_note.recipient().clone()])
            .unauthenticated_input_notes(vec![(note, None)])
            // .own_output_notes(vec![OutputNote::Full(expected_bridge_note)])
            .build()?;

        info!("Perform transaction");
        let tx = self.client.new_transaction(account_id, tx_request).await?;

        let tx_id = tx.executed_transaction().id();
        info!("Executed Tx on MidenScan: https://testnet.midenscan.com/tx/{:?}", tx_id);
        // TODO: check somehow note commitment

        self.client.submit_transaction(tx).await?;
        info!("Submitted transaction");

        self.cleanup().await?;

        Ok(tx_id.to_hex())
    }

    pub async fn is_note_onchain(&mut self, note_id: NoteId) -> Result<bool, MixerClientError> {
        self.client.sync_state().await?;

        Ok(self.rpc.get_note_by_id(note_id).await.is_ok())
    }

    // TODO: result type should not neglect about individual note failures, so we can identify
    // errorneous notes and ignore them
    #[tracing::instrument(skip_all)]
    pub async fn mix_batch(
        &mut self,
        notes: Vec<Note>,
        account_id: AccountId,
    ) -> Result<TransactionId, MixerClientError> {
        if notes.len() > MAX_NOTES_IN_BATCH_TRANSACTION {
            return Err(MixerClientError::TransactionNotesLimit());
        }

        self.check_crosschain_notes(&notes).await?;
        self.check_accounts_manageable(&vec![account_id]).await?;

        // import notes to client store
        let _note_ids = self.import_notes(&notes).await?;

        // reconstruct expected note from the bridge
        let expected_bridge_notes: Vec<Note> =
            notes.iter().map(get_public_bridge_output_note).collect::<Result<Vec<_>, _>>()?;
        let expected_output_recipients =
            expected_bridge_notes.iter().map(|note| note.recipient()).cloned().collect();

        // sync state
        self.client.sync_state().await?;

        info!("Build tx request");
        let unauth_notes: Vec<(Note, Option<NoteArgs>)> =
            notes.into_iter().map(|n| (n, None)).collect();
        let tx_request = TransactionRequestBuilder::new()
            .expected_output_recipients(expected_output_recipients)
            .unauthenticated_input_notes(unauth_notes)
            .build()?;

        info!("Execute tx");
        let tx = self.client.new_transaction(account_id, tx_request).await?;
        info!("Tx finished");

        let tx_id = tx.executed_transaction().id();
        info!("Executed Tx on MidenScan: https://testnet.midenscan.com/tx/{:?}", tx_id);

        self.client.submit_transaction(tx).await?;
        info!("Tx was submitted");

        self.cleanup().await?;

        Ok(tx_id)
    }

    async fn check_crosschain_notes(&mut self, notes: &Vec<Note>) -> Result<(), MixerClientError> {
        for note in notes {
            if note.recipient().script().root() == croschain().root() {
                Ok(())
            } else {
                Err(MixerClientError::WrongNoteScriptRootError())
            }?;
        }

        Ok(())
    }

    async fn check_accounts_manageable(
        &mut self,
        account_ids: &Vec<AccountId>,
    ) -> Result<(), MixerClientError> {
        self.client.sync_state().await?;

        for account_id in account_ids {
            let account = self.client.try_get_account(*account_id).await;

            if let Err(MidenClientError::AccountDataNotFound(_)) = account {
                return Err(MixerClientError::NotManageableAccountError(account_id.to_hex()));
            };
        }

        Ok(())
    }

    async fn import_notes(&mut self, notes: &Vec<Note>) -> Result<Vec<NoteId>, MixerClientError> {
        self.client.sync_state().await?;

        let mut note_ids = Vec::new();
        for note in notes {
            // obtain a cryptographic proof that note exists within the blockchain's state
            let fetched_note = self.rpc.get_note_by_id(note.id()).await?;

            let note_file =
                NoteFile::NoteWithProof(note.clone(), fetched_note.inclusion_proof().clone());

            let note_id = self.client.import_note(note_file).await?;

            note_ids.push(note_id);
        }

        Ok(note_ids)
    }
}

#[cfg(test)]
mod test {
    use miden_client::note::BlockNumber;
    use tempfile::NamedTempFile;

    use super::*;

    struct Fixture {
        rpc_url: String,
        store_file: NamedTempFile,
        private_account_dir: PathBuf,
        public_account_ids: Vec<String>,
    }

    impl Fixture {
        // fn from_env() -> Self {
        //     dotenvy::dotenv().ok();
        //     let rpc_url = std::env::var("MO_TEST_RPC_ENDPOINT_URL").expect("Missing
        // MO_RPC_ENDPOINT_URL");     let store_file =
        // NamedTempFile::new().expect("NamedTempFile");     Fixture::new(rpc_url,
        // store_file) }

        fn from_config() -> Self {
            let config = rocket::Config::figment()
                .extract::<crate::config::Config>()
                .expect("reading figment provided config");

            if cfg!(debug_assertions) {
                tracing::info!("Loaded test config:\n{config:#?}");
            }

            let store_file = NamedTempFile::new().expect("NamedTempFile");

            Fixture::new(
                config.client().rpc_url(),
                store_file,
                config.client().private_account_dir(),
                config.client().public_account_ids(),
            )
        }

        fn new(
            rpc_url: String,
            store_file: NamedTempFile,
            private_account_dir: PathBuf,
            public_account_ids: Vec<String>,
        ) -> Self {
            Fixture {
                rpc_url,
                store_file,
                private_account_dir,
                public_account_ids,
            }
        }

        pub fn rpc_url(&self) -> &str {
            self.rpc_url.as_str()
        }

        pub fn rpc_timeout_ms(&self) -> u64 {
            10000
        }

        pub fn store_file_path(&self) -> PathBuf {
            self.store_file.path().to_path_buf()
        }

        pub fn private_account_dir(&self) -> PathBuf {
            self.private_account_dir.clone()
        }

        pub fn public_account_ids(&self) -> Vec<String> {
            self.public_account_ids.clone()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rpc_get_block_with_tokio_rt() {
        let fixture = Fixture::from_config();

        let rpc = Arc::new(TonicRpcClient::new(
            &Endpoint::try_from(fixture.rpc_url()).unwrap(),
            fixture.rpc_timeout_ms(),
        ));

        assert!(rpc.get_block_by_number(BlockNumber::GENESIS).await.is_ok())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_client_sync_state_with_tokio_rt() {
        let fixture = Fixture::from_config();

        let store = SqliteStore::new(fixture.store_file_path()).await.expect("SqliteStore::new");

        let store = Arc::new(store);

        let mut rng = rng();
        let coin_seed: [u64; 4] = rng.random();

        let rng = RpoRandomCoin::new(Word::from(coin_seed.map(Felt::new)));

        let rpc = Arc::new(TonicRpcClient::new(
            &Endpoint::try_from(fixture.rpc_url()).unwrap(),
            fixture.rpc_timeout_ms(),
        ));

        let mut client = MidenClient::new(
            rpc.clone(),
            Box::new(rng),
            store.clone() as Arc<dyn Store>,
            Some(Arc::new(BasicAuthenticator::<StdRng>::new(&[]))),
            ExecutionOptions::new(
                Some(MAX_TX_EXECUTION_CYCLES),
                MIN_TX_EXECUTION_CYCLES,
                true,
                true,
            )
            .unwrap(),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(client.sync_state().await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_mixer_client_with_tokio_rt() {
        let fixture = Fixture::from_config();

        let mut mixer_client =
            MixerClient::new(fixture.rpc_url(), fixture.rpc_timeout_ms(), None, true)
                .await
                .expect("MixerClient::new");

        mixer_client
            .initialize(fixture.private_account_dir(), fixture.public_account_ids())
            .await
            .expect("mixer_client.initialize");

        // TODO: to test mix(), need to create `note` and `account_id` somehow
        // let note = Note::new();
        // let account_id = AccountId::dummy(elements)
        // assert!(mixer_client.mix().await.is_ok())
    }
}

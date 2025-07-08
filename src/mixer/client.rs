use rand::{Rng, rng};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs::read;
use tracing::info;

use glob::glob;
use miden_bridge::{
    accounts::token_wrapper::bridge_note_tag,
    notes::bridge::{bridge, croschain},
};
use miden_client::{
    Client as MidenClient, ClientError as MidenClientError,
    rpc::{Endpoint, TonicRpcClient},
    store::{Store, sqlite_store::SqliteStore},
    transaction::{TransactionRequestBuilder, TransactionRequestError},
};
use miden_objects::{
    AccountIdError, Felt, NoteError, Word, ZERO,
    account::{AccountFile, AccountId},
    asset::Asset,
    crypto::rand::RpoRandomCoin,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteFile, NoteInputs, NoteMetadata, NoteRecipient,
        NoteType,
    },
    transaction::OutputNote,
    utils::{Deserializable, DeserializationError},
};

const DEFAULT_STORAGE_FILE: &str = "store.db";

pub struct MixerClient {
    client: MidenClient,
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
}

impl MixerClient {
    pub async fn new(
        rpc_endpoint: &str,
        rpc_timeout_ms: u64,
        store_filename: Option<PathBuf>,
    ) -> Result<Self, MixerClientError> {
        let store = SqliteStore::new(
            store_filename
                .or(Some(PathBuf::from(DEFAULT_STORAGE_FILE.to_string())))
                .unwrap(),
        )
        .await
        .map_err(MidenClientError::StoreError)?;

        let store = Arc::new(store);

        let mut rng = rng();
        let coin_seed: [u64; 4] = rng.random();

        let rng = RpoRandomCoin::new(coin_seed.map(Felt::new));

        let client = MidenClient::new(
            Arc::new(TonicRpcClient::new(
                &Endpoint::try_from(rpc_endpoint)
                    .map_err(MixerClientError::MalformedEndpointUrlError)?,
                rpc_timeout_ms,
            )),
            Box::new(rng),
            store.clone() as Arc<dyn Store>,
            Arc::new(()),
            false,
            "".to_string(),
            None,
            None,
        );

        Ok(Self { client })
    }

    pub async fn initialize(
        &mut self,
        supported_accounts_dir: PathBuf,
        public_accounts_to_import: Vec<String>,
    ) -> Result<(), MixerClientError> {
        let mut supported_accounts_dir = supported_accounts_dir.to_str().unwrap().to_string();
        supported_accounts_dir.push_str("/*.mac");

        info!("Mixer operator initialization start");

        self.client.sync_state().await?;

        info!("Mixer state synced");

        for path in glob(supported_accounts_dir.as_str())
            .unwrap()
            .filter_map(Result::ok)
        {
            let account_bytes = read(path).await?;
            let account_file = AccountFile::read_from_bytes(account_bytes.as_slice())?;
            let account_id = account_file.account.id();
            let account_id_hex = account_id.to_hex();
            info!("Importing the private account with id {account_id_hex}");

            if self
                .client
                .try_get_account_header(account_id)
                .await
                .is_err()
            {
                self.client
                    .add_account(&account_file.account, None, false)
                    .await?;
            }
            info!("Private account imported")
        }

        for public_account_id in public_accounts_to_import {
            info!("Importing the public account with id {public_account_id}");
            let public_account_id = AccountId::from_hex(public_account_id.as_str())?;

            if self
                .client
                .try_get_account_header(public_account_id)
                .await
                .is_err()
            {
                self.client.import_account_by_id(public_account_id).await?;
            }
            info!("Public account imported")
        }

        Ok(())
    }

    async fn cleanup(&self) -> Result<(), MixerClientError> {
        // self.store.remove_notes().await?;

        Ok(())
    }

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

        // obtain a cryptographic proof that note exists within the blockchain's state
        let proof = self
            .client
            .get_note_inclusion_proof(note.id())
            .await?
            .ok_or(MixerClientError::InvalidNoteTypeError())?;

        let note_file = NoteFile::NoteWithProof(note.clone(), proof);

        let note_id = self.client.import_note(note_file).await?;

        // obtain account to consume to
        let account = self.client.try_get_account(account_id.clone()).await;

        // TODO: errors cast
        if let Err(MidenClientError::AccountDataNotFound(_)) = account {
            Err(MixerClientError::NotManageableAccountError(
                account_id.clone().to_hex(),
            ))
        } else {
            Ok(())
        }?;

        // sync state
        self.client.sync_state().await?;

        let tx = self
            .client
            .new_transaction(
                account_id,
                TransactionRequestBuilder::new()
                    .with_own_output_notes(vec![expected_bridge_note])
                    .with_empty_script(true)
                    .build_consume_notes(vec![note_id])?,
            )
            .await?;
        info!("Built transaction");

        let tx_id = tx.executed_transaction().id();

        self.client.submit_transaction(tx).await?;
        info!("Submit transaction");

        self.cleanup().await?;

        Ok(tx_id.to_hex())
    }
}

#[derive(Error, Debug)]
pub enum PublicNoteConstructorError {
    #[error("Fungible asset in the crosschain note is not found")]
    FungibleAssetNotFound(),
    #[error(transparent)]
    NoteCreationError(#[from] NoteError),
    #[error("Malformed serial number")]
    MalformedSerialNumber(),
}

fn get_public_bridge_output_note(
    input_note: &Note,
) -> Result<OutputNote, PublicNoteConstructorError> {
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

    let serial_num = Word::try_from(input_note.inputs().values()[..4].to_vec())
        .map_err(|_| PublicNoteConstructorError::MalformedSerialNumber())?;

    let inputs = NoteInputs::new(
        vec![
            Word::from(Asset::Fungible(crosschain_asset.clone())).to_vec(),
            input_note.inputs().values()[4..].to_vec(),
        ]
        .concat(),
    )?;

    let recipient = NoteRecipient::new(serial_num, script, inputs);

    Ok(OutputNote::Full(Note::new(assets, metadata, recipient)))
}

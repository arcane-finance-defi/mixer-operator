use std::path::PathBuf;
use miden_client::{Client, ClientError};
//use miden_client::store::sqlite_store::SqliteStore;
use thiserror::Error;
use std::sync::Arc;
use miden_bridge::accounts::token_wrapper::bridge_note_tag;
use miden_client::rpc::{Endpoint, TonicRpcClient};
use miden_client::store::Store;
use miden_client::transaction::{TransactionRequestBuilder, TransactionRequestError};
use miden_objects::account::{AccountFile, AccountId};
use miden_objects::crypto::rand::RpoRandomCoin;
use miden_objects::{AccountIdError, Felt, NoteError, Word, ZERO};
use miden_objects::note::{Note, NoteAssets, NoteExecutionHint, NoteFile, NoteInputs, NoteMetadata, NoteRecipient, NoteType};
use miden_bridge::notes::bridge::{croschain, bridge};
use miden_objects::asset::Asset;
use miden_objects::transaction::OutputNote;
use miden_objects::utils::{Deserializable, DeserializationError};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use js_sys::{Array, Uint8Array};
use miden_client::store::web_store::WebStore;
use web_sys::{console, window};

// Импорты для работы с IndexedDB вместо SQLite
use wasm_bindgen::JsCast;
use web_sys::{IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransaction, IdbObjectStore};

const DEFAULT_STORAGE_NAME: &str = "miden_mixer_store";

#[wasm_bindgen]
pub struct MixerClient {
    client: Option<Client>,
    storage_name: String,
}

#[derive(Error, Debug)]
pub enum MixerClientError {
    #[error(transparent)]
    InternalClientError(#[from] ClientError),
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
    InternalDeserializationError(#[from] DeserializationError),
    #[error(transparent)]
    AccountIdParsingError(#[from] AccountIdError),
    #[error("JavaScript error: {0}")]
    JsError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
}

impl From<JsValue> for MixerClientError {
    fn from(err: JsValue) -> Self {
        MixerClientError::JsError(format!("{:?}", err))
    }
}

#[wasm_bindgen]
impl MixerClient {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self {
            client: None,
            storage_name: DEFAULT_STORAGE_NAME.to_string(),
        }
    }

    #[wasm_bindgen]
    pub async fn initialize_client(&mut self, rpc_endpoint: &str, rpc_timeout_ms: u64) -> Result<(), JsValue> {
        self.init_client_internal(rpc_endpoint, rpc_timeout_ms)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn initialize_accounts(&mut self, private_accounts: Array, public_accounts_to_import: Array) -> Result<(), JsValue> {
        self.init_accounts_internal(private_accounts, public_accounts_to_import)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn sync_state(&mut self) -> Result<(), JsValue> {
        self.sync_state_internal()
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn mix(&mut self, note_file_bytes: &[u8], account_id_hex: &str) -> Result<String, JsValue> {
        self.mix_internal(note_file_bytes, account_id_hex)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

impl MixerClient {
    async fn init_client_internal(&mut self, rpc_endpoint: &str, rpc_timeout_ms: u64) -> Result<(), MixerClientError> {
        console::log_1(&"Initializing Mixer client...".into());

        // IndexedDB адаптер
        let db_name = self.storage_name.clone();
        let store = WebStore::new()
            .await
            .map_err(|e| MixerClientError::StorageError(format!("{:?}", e)))?;
        let store = Arc::new(store);

        // Генерация seed
        let mut buf = [0u8; 32];
        let win = window().ok_or_else(|| MixerClientError::JsError("No window object".into()))?;
        let crypto = win.crypto().map_err(|_| MixerClientError::JsError("No crypto".into()))?;
        crypto.get_random_values_with_u8_array(&mut buf)
            .map_err(|e| MixerClientError::JsError(format!("Crypto error: {:?}", e)))?;
        let seed: [u64; 4] = buf
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let rng = RpoRandomCoin::new(seed.map(Felt::new));

        let endpoint = Endpoint::try_from(rpc_endpoint)
            .map_err(|e| MixerClientError::MalformedEndpointUrlError(e.to_string()))?;

        let client = Client::new(
            Arc::new(TonicRpcClient::new(&endpoint, rpc_timeout_ms)),
            Box::new(rng),
            store as Arc<dyn Store>,
            Arc::new(()),
            false,
            "".to_string(),
            None,
            None,
        );

        self.client = Some(client);
        console::log_1(&"Mixer client initialized successfully".into());
        Ok(())
    }

    async fn init_accounts_internal(&mut self, private_accounts: Array, public_accounts_to_import: Array) -> Result<(), MixerClientError> {
        console::log_1(&"Starting account initialization...".into());

        // sync
        self.client.as_mut().ok_or_else(|| MixerClientError::JsError("Client not initialized".into()))?
            .sync_state().await?;
        console::log_1(&"State synced".into());

        // приватные
        for i in 0..private_accounts.length() {
            let item = private_accounts.get(i);
            if let Ok(arr) = item.dyn_into::<Uint8Array>() {
                let bytes = arr.to_vec();
                self.import_private_account_from_bytes(bytes).await?;
            }
        }

        // публичные
        for i in 0..public_accounts_to_import.length() {
            let id_val = public_accounts_to_import.get(i);
            if let Some(id_str) = id_val.as_string() {
                let account_id = AccountId::from_hex(&id_str)?;
                let client = self.client.as_mut().unwrap();
                if client.try_get_account_header(account_id).await.is_err() {
                    client.import_account_by_id(account_id).await?;
                }
                console::log_1(&"Public account imported".into());
            }
        }
        Ok(())
    }

    async fn import_private_account_from_bytes(&mut self, bytes: Vec<u8>) -> Result<(), MixerClientError> {
        let client = self.client.as_mut().ok_or_else(|| MixerClientError::JsError("Client not initialized".into()))?;
        let file = AccountFile::read_from_bytes(&bytes)?;
        let id = file.account.id();
        console::log_1(&format!("Importing private account: {}", id.to_hex()).into());
        if client.try_get_account_header(id).await.is_err() {
            client.add_account(&file.account, None, false).await?;
        }
        console::log_1(&"Private account imported".into());
        Ok(())
    }

    async fn sync_state_internal(&mut self) -> Result<(), MixerClientError> {
        self.client.as_mut().ok_or_else(|| MixerClientError::JsError("Client not initialized".into()))?
            .sync_state().await?;
        Ok(())
    }

    async fn mix_internal(&mut self, note_file_bytes: &[u8], account_id_hex: &str) -> Result<String, MixerClientError> {
        let client = self.client.as_mut().ok_or_else(|| MixerClientError::JsError("Client not initialized".into()))?;
        let note_file = NoteFile::read_from_bytes(note_file_bytes)?;
        let note = match note_file {
            NoteFile::NoteWithProof(n, _) => n,
            _ => return Err(MixerClientError::InvalidNoteTypeError()),
        };
        if note.recipient().script().root() != croschain().root() {
            return Err(MixerClientError::WrongNoteScriptRootError());
        }
        let expected = get_public_bridge_output_note(&note)?;

        client.sync_state().await?;
        let proof = client.get_note_inclusion_proof(note.id()).await?.ok_or(MixerClientError::InvalidNoteTypeError())?;
        let nf = NoteFile::NoteWithProof(note.clone(), proof);
        let note_id = client.import_note(nf).await?;
        let account_id = AccountId::from_hex(account_id_hex)?;
        if let Err(ClientError::AccountDataNotFound(_)) = client.try_get_account(account_id).await {
            return Err(MixerClientError::NotManageableAccountError(account_id_hex.to_string()));
        }
        client.sync_state().await?;
        let tx = client.new_transaction(
            account_id,
            TransactionRequestBuilder::new()
                .with_own_output_notes(vec![expected])
                .with_empty_script(true)
                .build_consume_notes(vec![note_id])?
        ).await?;
        let tx_id = tx.executed_transaction().id();
        client.submit_transaction(tx).await?;
        Ok(tx_id.to_hex())
    }

    async fn create_wasm_store(&self) -> Result<impl Store, MixerClientError> {
        let name = self.storage_name.clone();
        WebStore::new()
            .await
            .map_err(|e| MixerClientError::StorageError(format!("{:?}", e)))
    }
}

// Функция для генерации криптографически стойкого seed в WASM
// Функция для генерации криптографически стойкого seed в WASM
fn generate_secure_random_seed() -> Result<[u64; 4], MixerClientError> {
    let window = window()
        .ok_or_else(|| MixerClientError::JsError("No window object".to_string()))?;

    let crypto = window.crypto()
        .map_err(|_| MixerClientError::JsError("No crypto object".to_string()))?;

    // Генерируем 32 байта непосредственно в Rust-массив
    let mut buf = [0u8; 32];
    crypto.get_random_values_with_u8_array(&mut buf)
        .map_err(|e| MixerClientError::JsError(format!("Failed to get random values: {:?}", e)))?;

    // Конвертируем байты в четыре u64 (little endian)
    let mut seed = [0u64; 4];
    for i in 0..4 {
        let start = i * 8;
        let chunk = &buf[start..start + 8];
        seed[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }

    Ok(seed)
}


#[derive(Error, Debug)]
pub enum PublicNoteConstructorError {
    #[error("Fungible asset in the crosschain note not found")]
    FungibleAssetNotFound(),
    #[error(transparent)]
    NoteCreationError(#[from] NoteError),
    #[error("Malformed serial number")]
    MalformedSerialNumber(),
}

fn get_public_bridge_output_note(input_note: &Note) -> Result<OutputNote, PublicNoteConstructorError> {
    let crosschain_asset = input_note.assets().iter().last()
        .ok_or(PublicNoteConstructorError::FungibleAssetNotFound())?;

    let crosschain_asset = match crosschain_asset {
        Asset::Fungible(asset) => Ok(asset),
        _ => Err(PublicNoteConstructorError::FungibleAssetNotFound())
    }?;

    let script = bridge();
    let assets = NoteAssets::default();
    let metadata = NoteMetadata::new(
        crosschain_asset.faucet_id(),
        NoteType::Public,
        bridge_note_tag(),
        NoteExecutionHint::Always,
        ZERO
    )?;

    let serial_num = Word::try_from(input_note.inputs().values()[..4].to_vec())
        .map_err(|_| PublicNoteConstructorError::MalformedSerialNumber())?;

    let inputs = NoteInputs::new(
        vec![
            Word::from(Asset::Fungible(crosschain_asset.clone())).to_vec(),
            input_note.inputs().values()[4..].to_vec()
        ].concat()
    )?;

    let recipient = NoteRecipient::new(
        serial_num,
        script,
        inputs
    );

    Ok(OutputNote::Full(Note::new(
        assets,
        metadata,
        recipient
    )))
}

// Экспорт функции для установки panic hook
/*#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
}*/
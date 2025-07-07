use std::path::PathBuf;
use miden_client::{Client, ClientError};
use miden_client::store::sqlite_store::SqliteStore;
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
use js_sys::{Array, Promise, Uint8Array};
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
    pub async fn initialize_client(
        &mut self,
        rpc_endpoint: &str,
        rpc_timeout_ms: u64,
    ) -> Result<(), JsValue> {
        let result = self.init_client_internal(rpc_endpoint, rpc_timeout_ms).await;
        result.map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn initialize_accounts(
        &mut self,
        private_accounts: Array,
        public_accounts_to_import: Array,
    ) -> Result<(), JsValue> {
        let result = self.init_accounts_internal(private_accounts, public_accounts_to_import).await;
        result.map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn sync_state(&mut self) -> Result<(), JsValue> {
        let result = self.sync_state_internal().await;
        result.map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn mix(&mut self, note_file_bytes: &[u8], account_id_hex: &str) -> Result<String, JsValue> {
        let result = self.mix_internal(note_file_bytes, account_id_hex).await;
        result.map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

impl MixerClient {
    async fn init_client_internal(
        &mut self,
        rpc_endpoint: &str,
        rpc_timeout_ms: u64,
    ) -> Result<(), MixerClientError> {
        console::log_1(&"Initializing Mixer client...".into());

        // Создаем простое хранилище в памяти для WASM
        // В production версии здесь должен быть IndexedDB адаптер
        let store = self.create_wasm_store().await?;
        let store = Arc::new(store);

        // Генерируем безопасный seed для WASM
        let coin_seed = generate_secure_random_seed()?;
        let rng = RpoRandomCoin::new(coin_seed.map(Felt::new));

        // Создаем endpoint
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
            None
        );

        self.client = Some(client);
        console::log_1(&"Mixer client initialized successfully".into());
        Ok(())
    }

    async fn init_accounts_internal(
        &mut self,
        private_accounts: Array,
        public_accounts_to_import: Array,
    ) -> Result<(), MixerClientError> {
        console::log_1(&"Starting account initialization...".into());

        let client = self.client.as_mut()
            .ok_or_else(|| MixerClientError::JsError("Client not initialized".to_string()))?;

        client.sync_state().await?;
        console::log_1(&"State synced".into());

        // Обрабатываем приватные аккаунты
        for i in 0..private_accounts.length() {
            let account_bytes = private_accounts.get(i);
            if let Ok(uint8_array) = account_bytes.dyn_into::<Uint8Array>() {
                let bytes = uint8_array.to_vec();
                self.import_private_account_from_bytes(bytes).await?;
            }
        }

        // Импортируем публичные аккаунты
        for i in 0..public_accounts_to_import.length() {
            let account_id = public_accounts_to_import.get(i);
            if let Some(account_id_str) = account_id.as_string() {
                console::log_1(&format!("Importing public account: {}", account_id_str).into());
                let public_account_id = AccountId::from_hex(&account_id_str)?;

                if client.try_get_account_header(public_account_id).await.is_err() {
                    client.import_account_by_id(public_account_id).await?;
                }
                console::log_1(&"Public account imported".into());
            }
        }

        Ok(())
    }

    async fn import_private_account_from_bytes(&mut self, account_bytes: Vec<u8>) -> Result<(), MixerClientError> {
        let client = self.client.as_mut()
            .ok_or_else(|| MixerClientError::JsError("Client not initialized".to_string()))?;

        let account_file = AccountFile::read_from_bytes(&account_bytes)?;
        let account_id = account_file.account.id();
        let account_id_hex = account_id.to_hex();

        console::log_1(&format!("Importing private account: {}", account_id_hex).into());

        if client.try_get_account_header(account_id).await.is_err() {
            client.add_account(&account_file.account, None, false).await?;
        }

        console::log_1(&"Private account imported".into());
        Ok(())
    }

    async fn sync_state_internal(&mut self) -> Result<(), MixerClientError> {
        let client = self.client.as_mut()
            .ok_or_else(|| MixerClientError::JsError("Client not initialized".to_string()))?;

        client.sync_state().await?;
        Ok(())
    }

    async fn mix_internal(&mut self, note_file_bytes: &[u8], account_id_hex: &str) -> Result<String, MixerClientError> {
        let client = self.client.as_mut()
            .ok_or_else(|| MixerClientError::JsError("Client not initialized".to_string()))?;

        // Десериализуем NoteFile из байтов
        let note_file = NoteFile::read_from_bytes(note_file_bytes)?;
        let account_id = AccountId::from_hex(account_id_hex)?;

        let note = match note_file {
            NoteFile::NoteWithProof(ref note, _) => Ok(note),
            _ => Err(MixerClientError::InvalidNoteTypeError())
        }?;

        // Проверяем script root
        if note.recipient().script().root() != croschain().root() {
            return Err(MixerClientError::WrongNoteScriptRootError());
        }

        let expected_bridge_note = get_public_bridge_output_note(note)?;

        client.sync_state().await?;

        let proof = client.get_note_inclusion_proof(note.id()).await?
            .ok_or(MixerClientError::InvalidNoteTypeError())?;

        let note_file = NoteFile::NoteWithProof(
            note.clone(),
            proof,
        );

        let note_id = client.import_note(note_file).await?;

        // Проверяем, что аккаунт управляемый
        let account = client.try_get_account(account_id).await;
        if let Err(ClientError::AccountDataNotFound(_)) = account {
            return Err(MixerClientError::NotManageableAccountError(account_id.to_hex()));
        }

        client.sync_state().await?;

        let tx = client.new_transaction(
            account_id,
            TransactionRequestBuilder::new()
                .with_own_output_notes(vec![expected_bridge_note])
                .with_empty_script(true)
                .build_consume_notes(vec![note_id])?
        ).await?;

        let tx_id = tx.executed_transaction().id();

        client.submit_transaction(tx).await?;
        self.cleanup().await?;

        Ok(tx_id.to_hex())
    }

    async fn cleanup(&self) -> Result<(), MixerClientError> {
        // Очистка для WASM - пока заглушка
        Ok(())
    }

    // Создаем заглушку для Store в WASM
    // В production версии здесь должен быть полноценный IndexedDB адаптер
    async fn create_wasm_store(&self) -> Result<impl Store, MixerClientError> {
        // Для простоты используем SQLite с in-memory базой
        // В production версии замените на IndexedDB адаптер
        SqliteStore::new(PathBuf::from(":memory:")).await
            .map_err(|e| MixerClientError::StorageError(e.to_string()))
    }
}

// Функция для генерации криптографически стойкого seed в WASM
fn generate_secure_random_seed() -> Result<[u64; 4], MixerClientError> {
    let window = window()
        .ok_or_else(|| MixerClientError::JsError("No window object".to_string()))?;

    let crypto = window.crypto()
        .map_err(|_| MixerClientError::JsError("No crypto object".to_string()))?;

    let array = Uint8Array::new_with_length(32);
    crypto.get_random_values_with_u8_array(&mut array.clone())
        .map_err(|_| MixerClientError::JsError("Failed to generate random bytes".to_string()))?;

    let bytes = array.to_vec();

    // Конвертируем байты в u64 (little endian)
    let mut seed = [0u64; 4];
    for i in 0..4 {
        let start = i * 8;
        seed[i] = u64::from_le_bytes([
            bytes[start], bytes[start + 1], bytes[start + 2], bytes[start + 3],
            bytes[start + 4], bytes[start + 5], bytes[start + 6], bytes[start + 7],
        ]);
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
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
}
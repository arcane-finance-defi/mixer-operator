use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use hex::{decode, FromHexError};
use miden_objects::account::AccountId;
use miden_objects::note::NoteFile;
use miden_objects::utils::DeserializationError;
use miden_objects::AccountIdError;
/*use crate::config::Config;
use crate::mixer::client::MixerClientError;
use thiserror::Error;
//use js_sys::Uint8Array;

mod config;
mod mixer;

#[derive(Error, Debug)]
pub enum EndpointError {
    #[error(transparent)]
    HexError(#[from] FromHexError),
    #[error(transparent)]
    DeserializationError(#[from] DeserializationError),
    #[error(transparent)]
    AccountIdError(#[from] AccountIdError),
    #[error(transparent)]
    MixerClientError(#[from] MixerClientError),
}

#[derive(Debug, Deserialize, Serialize)]
#[wasm_bindgen]
pub struct MixRequest {
    note_text: String,
    account_id: String,
}

#[wasm_bindgen]
impl MixRequest {
    #[wasm_bindgen(constructor)]
    pub fn new(note_text: String, account_id: String) -> MixRequest {
        MixRequest {
            note_text,
            account_id,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn note_text(&self) -> String {
        self.note_text.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn account_id(&self) -> String {
        self.account_id.clone()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[wasm_bindgen]
pub struct MixResponse {
    tx_id: String,
}

#[wasm_bindgen]
impl MixResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(tx_id: String) -> MixResponse {
        MixResponse { tx_id }
    }

    #[wasm_bindgen(getter)]
    pub fn tx_id(&self) -> String {
        self.tx_id.clone()
    }
}

#[wasm_bindgen]
pub struct MixerState {
    client: mixer::client::MixerClient,
}

#[wasm_bindgen]
impl MixerState {
    #[wasm_bindgen(constructor)]
    pub async fn new(config: Config) -> Result<MixerState, JsValue> {
        let mut client = mixer::client::MixerClient::new(
            &config.rpc_url(),
            config.rpc_timeout_ms(),
            None,
        ).await.map_err(|e| JsValue::from_str(&format!("Client creation error: {}", e)))?;

        // Инициализируем с пустыми аккаунтами - они будут добавлены через отдельные методы
        client.initialize(
            Vec::new(),
            config.public_account_ids(),
        ).await.map_err(|e| JsValue::from_str(&format!("Client initialization error: {}", e)))?;

        Ok(MixerState { client })
    }

    // Добавляем метод для импорта приватных аккаунтов
    #[wasm_bindgen]
    pub async fn import_private_account(&mut self, account_data: &vec![0u8; 32]) -> Result<(), JsValue> {
        let mut bytes = vec![0u8; account_data.length() as usize];
        account_data.copy_to(&mut bytes);

        self.client.import_private_account_from_bytes(bytes).await
            .map_err(|e| JsValue::from_str(&format!("Private account import error: {}", e)))
    }

    #[wasm_bindgen]
    pub async fn mix(&mut self, data: MixRequest) -> Result<MixResponse, JsValue> {
        let note_bytes = decode(&data.note_text)
            .map_err(|e| JsValue::from_str(&format!("Hex decode error: {}", e)))?;

        let note_file = NoteFile::read_from_bytes(note_bytes.as_slice())
            .map_err(|e| JsValue::from_str(&format!("Note file error: {}", e)))?;

        let account_id = AccountId::from_hex(&data.account_id)
            .map_err(|e| JsValue::from_str(&format!("Account ID error: {}", e)))?;

        let response = self.client.mix(note_file, account_id).await
            .map_err(|e| JsValue::from_str(&format!("Mix error: {}", e)))?;

        Ok(MixResponse {
            tx_id: response
        })
    }

    #[wasm_bindgen]
    pub async fn sync_state(&mut self) -> Result<(), JsValue> {
        self.client.sync_state().await
            .map_err(|e| JsValue::from_str(&format!("Sync error: {}", e)))
    }
}
*/
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"WASM module initialized".into());
}
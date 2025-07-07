# Miden mixer operator

Mixer operator is rust based offchain service that generates the consume-note transactions with provided CROSSCHAIN notes and preconfigured public faucet accounts.

## API

The service provides wasm mixer-operator that generates tx from the note and account and returns tx id

* rust v1.87.0

## How to build

Start the service with the cmd

```
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' wasm-pack build --target web --out-dir pkg

```
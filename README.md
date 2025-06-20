# Miden mixer operator

Mixer operator is rust based offchain service that generates the consume-note transactions with provided CROSSCHAIN notes and preconfigured public faucet accounts.

## API

The service provides singe endpoint `POST /mix` that generates tx from the note and account and returns tx id

## Configuration

___./Rocket.toml___ contains the configs for the service

### Config keys

* ___rpc_url___ URL of miden node GRPC api
* ___rpc_timeout_ms___ miden rpc request timeout in milliseconds
* ___public_account_ids___ comma separated list of public faucet accounts on miden chain to work with

## Prerequisites

* rust v1.87.0

## How to run

Start the service with the cmd

```
cargo run --release
```
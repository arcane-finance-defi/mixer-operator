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

* rust v1.88.0

## How to run

Start the service with the cmd

```
cargo run --release
```

## How to deploy

1. Build the service with target `x86_64-unknown-linux-gnu`
2. Connect to the server via SSH `ssh root@156.67.63.214`
3. Stop the previous version `killall mixer-operator`
4. Copy the binaries to the server `scp ./target/x86_64-unknown-linux-gnu/release/mixer-operator root@156.67.63.214:/root/mixer/mixer-operator`
5. Start the service `cd ./mixer && nohup ./mixer-operator &`

## How to test

### Test prerequisites

* Latest version of [miden-bridge CLI](https://github.com/arcane-finance-defi/miden-bridge-cli) (Install with `cargo install --git https://github.com/arcane-finance-defi/miden-bridge-cli miden-cli` command)
* [Foundry](https://getfoundry.sh/) toolchain

1. Cleanup previous cli configs `rm -r miden-client.toml store.sqlite3 templates keystore`
2. Fill _.env_ file. `cp .env.example .env` and fill the _TEST_PRIVATE_KEY_ env var with EVM private key of the source test account, _TEST_RECEIVER_ADDRESS_ with public EVM address of target account, _TEST_USDC_AMOUNT_ to specify custom amount of USDC tokens to be mixed. __DO NOT USE THE ACCOUNT THAT HOLDS ANY REAL ASSETS. THE PRIVATE KEY WILL BE INCLUDED INTO THE TEST LOGS__
3. Run test with `cargo test --package mixer-operator --test mixing_flow test_usdc_mixing_flow -- --exact` (may take some time)
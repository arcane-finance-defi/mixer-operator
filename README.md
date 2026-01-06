# Miden mixer operator

The mixer operator is a Rust-based off-chain service that generates consume-note transactions using the provided cross-chain notes and preconfigured public faucet accounts.
## API

The service provides:

* `POST /api/v1/mix` endpoint that generates tx from the note and account and returns `tx_id`
* `POST /api/v1/mix/batch` is the same as above but expect batch of requests
* `POST /api/v1/mix/delayed` endpoint that put note execution request into the queue to be excuted later with given delay in milliseconds and return `task_id`
* `POST /api/v1/mix/batch/delayed` is the same as above but expect batch of requests
* `GET /api/v1/mix/delayed/status/<task_id>` endpoint that returns note execution status for given `task_id`

The swagger-ui is accessible at `swagger-ui/api/v1` endpoint

## Configuration

___./Rocket.toml___ contains the configs for the service

### Config keys

* ___rpc_url___ URL of miden node GRPC api
* ___rpc_timeout_ms___ miden rpc request timeout in milliseconds
* ___public_account_ids___ comma separated list of public faucet accounts on miden chain to work with

* ___tq.db_url___ connection URL of task queue database
* ___db.url___ URL of local notes storage
## Prerequisites

* rust v1.89.0

## How to run

Start the service with the cmd

```
cargo run --release
```

Or with docker - see [docker deploy docs](/deploy/README.md)

## How to test

* To run unit-tests only run `cargo test --lib`
* To run with integration tests run `cargo test`. Do not forget to set [environment variables](###test-prerequisites)

### Test prerequisites

* Latest version of [miden-bridge CLI](https://github.com/arcane-finance-defi/miden-bridge-cli) (Install with `cargo install --git https://github.com/arcane-finance-defi/miden-bridge-cli miden-client-cli` command)
* [Foundry](https://getfoundry.sh/) toolchain

1. Cleanup previous cli configs `rm -r miden-client.toml store.sqlite3 templates keystore`
2. Fill _.env_ file. `cp .env.example .env` and fill the _TEST_PRIVATE_KEY_ env var with EVM private key of the source test account, _TEST_RECEIVER_ADDRESS_ with public EVM address of target account, _TEST_USDC_AMOUNT_ to specify custom amount of USDC tokens to be mixed. __DO NOT USE THE ACCOUNT THAT HOLDS ANY REAL ASSETS. THE PRIVATE KEY WILL BE INCLUDED INTO THE TEST LOGS__
3. Run test with `cargo test --package mixer-operator --test mixing_flow test_usdc_mixing_flow -- --exact` (may take some time)

## How to Deploy

### How to deploy docker images to stage

##### Using [hoister](deploy/docker-compose.stage.hoister.yml)

* Docker images are build automatically using CI/CD and tagged with short-sha commit tag.
* Create git tag with pattern `stage-*` on commit you want to deploy to stage.
* This will launch another CI/CD pipeline which will create docker image with tag `stage`.
* Once `stage` image will be pushed to docker registry, `hoister` will update and restart target app container automatically.

##### How to revert
* Launch pipeline with desired (e.g. previous) stage tag manually from Github Actions. Latest launched pipeline become `stage` because of image tag update.

### How to deploy binary package

1. Build the service with target `x86_64-unknown-linux-gnu`
2. Connect to the server via SSH `ssh root@156.67.63.214`
3. Stop the previous version `killall mixer-operator`
4. Copy the binaries to the server `scp ./target/x86_64-unknown-linux-gnu/release/mixer-operator root@156.67.63.214:/root/mixer/mixer-operator`
5. Start the service `cd ./mixer && nohup ./mixer-operator &`

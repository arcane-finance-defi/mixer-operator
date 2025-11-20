FROM lukemathwalker/cargo-chef:latest-rust-1.90-trixie AS chef
RUN mkdir -p -m 0600 ~/.ssh && ssh-keyscan github.com >> ~/.ssh/known_hosts
WORKDIR /app

FROM chef AS planner
COPY . .
RUN --mount=type=ssh cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN --mount=type=ssh cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin mixer-operator

# We do not need the Rust toolchain to run the binary!
FROM debian:bookworm-slim AS runtime
RUN apt update && apt install -y libpq-dev ca-certificates
WORKDIR /app
COPY --from=builder /app/target/release/mixer-operator /usr/local/bin
COPY --from=builder /app/Rocket.toml /etc/mixer-operator/Rocket.toml
ENV ROCKET_CONFIG="/etc/mixer-operator/Rocket.toml"

ENTRYPOINT ["/usr/local/bin/mixer-operator"]

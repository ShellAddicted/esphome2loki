FROM lukemathwalker/cargo-chef:latest-rust-1.58.1 as chef
WORKDIR /app

FROM chef as planner
COPY . .
# Compute lock-like to build deps
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
COPY --from=planner /app/recipe.json recipe.json
# Build project dependencies
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
# Build project
RUN cargo build --release

FROM debian:buster-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/esphome2loki esphome2loki
ENV ESPHOME2LOKI_CONFIG="/config/config.toml"
ENTRYPOINT ["./esphome2loki"]
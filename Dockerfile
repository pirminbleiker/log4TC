# Stage 1: Cache dependencies (only rebuilds when Cargo.toml changes)
FROM rust:1.94 AS deps
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/log4tc-core/Cargo.toml crates/log4tc-core/Cargo.toml
COPY crates/log4tc-ads/Cargo.toml crates/log4tc-ads/Cargo.toml
COPY crates/log4tc-otel/Cargo.toml crates/log4tc-otel/Cargo.toml
COPY crates/log4tc-service/Cargo.toml crates/log4tc-service/Cargo.toml
COPY crates/log4tc-benches/Cargo.toml crates/log4tc-benches/Cargo.toml
COPY crates/log4tc-integration-tests/Cargo.toml crates/log4tc-integration-tests/Cargo.toml
RUN mkdir -p crates/log4tc-core/src && echo "pub fn dummy(){}" > crates/log4tc-core/src/lib.rs \
    && mkdir -p crates/log4tc-ads/src && echo "pub fn dummy(){}" > crates/log4tc-ads/src/lib.rs \
    && mkdir -p crates/log4tc-otel/src && echo "pub fn dummy(){}" > crates/log4tc-otel/src/lib.rs \
    && mkdir -p crates/log4tc-service/src && echo "fn main(){}" > crates/log4tc-service/src/main.rs \
    && mkdir -p crates/log4tc-benches/src && echo "pub fn dummy(){}" > crates/log4tc-benches/src/lib.rs \
    && mkdir -p crates/log4tc-integration-tests/src && echo "" > crates/log4tc-integration-tests/src/lib.rs
RUN cargo build --release -p log4tc-service 2>/dev/null; exit 0

# Stage 2: Build actual source (deps cached, only our code rebuilds)
FROM deps AS builder
COPY crates/ crates/
RUN touch crates/*/src/*.rs && cargo build --release -p log4tc-service

# Stage 3: Minimal runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/log4tc-service /usr/local/bin/
COPY config.docker.json /etc/log4tc/config.json
ENV LOG4TC_CONFIG=/etc/log4tc/config.json
EXPOSE 48898 16150
ENTRYPOINT ["log4tc-service"]

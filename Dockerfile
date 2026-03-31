FROM rust:1.94 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/log4tc-service /usr/local/bin/
COPY config.example.json /etc/log4tc/config.json
ENV LOG4TC_CONFIG=/etc/log4tc/config.json
EXPOSE 16150 4318
ENTRYPOINT ["log4tc-service"]

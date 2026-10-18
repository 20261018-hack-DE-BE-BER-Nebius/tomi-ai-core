FROM rust:1.99.1-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto ./proto
COPY src ./src
RUN cargo build --release --locked

FROM debian:12.13-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && useradd --uid 10001 --create-home tomi
WORKDIR /app
COPY --from=builder /src/target/release/tomi-ai-core /usr/local/bin/tomi-ai-core
COPY --from=builder /src/target/release/grpc_health_probe /usr/local/bin/grpc_health_probe
USER 10001
EXPOSE 8081 50051
HEALTHCHECK --interval=15s --timeout=5s --start-period=15s CMD curl -fsS http://127.0.0.1:8081/health || exit 1
CMD ["tomi-ai-core"]

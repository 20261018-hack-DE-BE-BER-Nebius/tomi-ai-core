FROM rust:1.99.1-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto ./proto
COPY src ./src
RUN cargo build --release --locked

FROM debian:12.13-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl ffmpeg && rm -rf /var/lib/apt/lists/* && useradd --uid 10001 --create-home tomi
RUN set -eu; case "$(dpkg --print-architecture)" in \
      amd64) nemo_arch=x86_64; nemo_hash=f396057150f1b774935c7414fd32ebc04195a795a34120d78d8c9ecefa1b7507 ;; \
      arm64) nemo_arch=aarch64; nemo_hash=ac50694d4729f9eae390a9851d38f51269d9af9829b535325539090a3b95c787 ;; \
      *) exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/v0.2.0/nemo-speech-0.2.0-linux-${nemo_arch}-cpu.tar.gz" -o /tmp/nemo.tar.gz; \
    echo "${nemo_hash}  /tmp/nemo.tar.gz" | sha256sum -c -; \
    mkdir -p /opt/nemo /opt/models; tar -xzf /tmp/nemo.tar.gz --strip-components=1 -C /opt/nemo; rm /tmp/nemo.tar.gz; \
    curl -fsSL "https://huggingface.co/nvidia/Nemotron-3-Diarization/resolve/f667ed73aee57d40cc39428eb768b4fd87a0a29e/Nemotron-3-Diarization.q8_0.gguf" -o /opt/models/Nemotron-3-Diarization.q8_0.gguf; \
    echo "08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1  /opt/models/Nemotron-3-Diarization.q8_0.gguf" | sha256sum -c -; \
    chmod 444 /opt/models/Nemotron-3-Diarization.q8_0.gguf; /opt/nemo/bin/nemo-speech --version
WORKDIR /app
COPY docs/model-notices /opt/models/notices
COPY --from=builder /src/target/release/tomi-ai-core /usr/local/bin/tomi-ai-core
COPY --from=builder /src/target/release/grpc_health_probe /usr/local/bin/grpc_health_probe
USER 10001
EXPOSE 8081 50051
HEALTHCHECK --interval=15s --timeout=5s --start-period=15s CMD curl -fsS http://127.0.0.1:8081/health || exit 1
CMD ["tomi-ai-core"]

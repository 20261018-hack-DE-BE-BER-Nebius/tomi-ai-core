# Tomi AI Core

Rust gateway for authenticated model inference, source comparisons, images, WebRTC sessions, and streaming agent traces. HTTP/WebSocket and gRPC use the same application layer. Provider credentials are read only by this service.

## Setup

Install Rust 1.99.1 and create `.env` from `.env.example`. Generate a random `TOMI_CORE_TOKEN` of at least 32 characters and share only that internal service token with trusted application services. Put provider credentials in this service's ignored `.env`; keep its permissions at `0600`. For production, inject them from Azure Key Vault using the container application's managed identity.

Set `OPENAI_BASE_URL` and `OPENAI_API_KEY`. Use `OPENAI_AUTH_HEADER=api-key` for an Azure OpenAI-compatible endpoint, or `bearer` for OpenAI. Set `NEBIUS_API_KEY` and the model endpoints to activate the required NVIDIA pipeline. Parse uses the explicit `nvidia/nemotron-3.5-parse` model. The case-sensitive agent deployment ID is `nvidia/Nemotron-3.5-Super-VL`.

`TOMI_HERMES_URL` identifies the source service. Its inbound service token must equal `TOMI_CORE_TOKEN`. Model, voice, bind-address, timeout, capacity, and image settings are documented in `.env.example` and loaded centrally by `src/config.rs`.

```sh
cargo run --bin tomi-ai-core
cargo run --bin grpc_health_probe -- -addr=127.0.0.1:50051
```

## Interfaces

All `/v1` routes require `Authorization: Bearer <TOMI_CORE_TOKEN>`. Provider keys never belong in browser requests. `/health` is process liveness; `/ready` reports configuration separately from verified inference.

| Route | Request and behavior |
| --- | --- |
| `GET /v1/models` | Live model-catalog discovery with explicit availability evidence |
| `POST /v1/chat/completions` | OpenAI-compatible request, including tool calls and SSE streaming; configured chat models only |
| `POST /v1/responses` | OpenAI Responses JSON or SSE for GPT agent function calls; output bounded, provider storage disabled |
| `POST /v1/embeddings`, `/v1/rerank`, `/v1/parse` | Required NVIDIA model services behind the core's credential boundary |
| `POST /v1/diarize` | Configured external diarization adapter with `audio` payload |
| `POST /v1/evidence/search` | `{query,language,top_k?}`; authenticated source retrieval for approved agent tools |
| `POST /v1/compare` | `{session_id,request_id,text,model,language,context}`; returns answer cards and actual trace events |
| `GET /v1/ws` | Bidirectional chat, steering, cancellation, and trace events |
| `POST /v1/images/generations` | `{prompt,model?,language?}`; returns provider image data |
| `POST /v1/realtime/session` | `{language,session_id?,model?}`; mints an ephemeral browser credential |
| `POST /v1/realtime/connect` | `{sdp,language,session_id?,model?}`; returns the provider SDP answer |

`language` is `en`, `es`, `de`, or `fr`. The browser should execute `compare_sources` voice-tool calls through authenticated application chat, then send the observed tool result back over the realtime data channel. Microphone mute uses the browser media-track control.

Comparison retrieval first extracts the encyclopedia subject as an English query. Citation, answer-format, and OCR-quality instructions remain in the final synthesis request. OpenAI agent clients should use the Responses route for Astra function tools. The route permits caller-executed function tools and disables provider-hosted tools and background execution.

Catalog probes for OpenAI, Nebius, and anonymous Parse run concurrently with an eight-second timeout each. A returned model ID verifies catalog presence; it does not guarantee a later invocation or available quota. A transient catalog error yields an unverified status for that request.

WebSocket commands are `chat.message`, `chat.steer` (`run_id`, `text`), `chat.cancel` (`run_id`), and `ping`. Steering aborts the previous run and creates a new run with preserved user context. The `run.steered` event records `previous_run_id`. A closed socket aborts its active work. Provider billing may continue for work already accepted upstream.

Outgoing `{type,run_id,session_id,timestamp,data}` events include `run.started`, `tool.started`, `tool.completed`, `answer.delta`, `answer.completed`, `run.failed`, `run.cancelled`, and `run.steered`. Comparison cards contain trusted component types and untrusted source text with source IDs, HTTPS provenance URLs, and visible uncertainty. The service never executes source-provided HTML, shell commands, or tools.

The HTTP listener also accepts native HTTP/2 gRPC on the same port, for Azure Container Apps ingress with `transport=http2`. A separate native gRPC listener remains available. The gRPC `tomi.ai.v1.AiGatewayService` service exposes `Chat` and `GetModels`; send the same bearer token as metadata. The checked-in `proto/ai.proto` builds independently with vendored protoc. The bundled `grpc_health_probe` binary performs the standard gRPC health check.

## Verification and containers

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo audit
python3 scripts/scan-secrets.py
git config core.hooksPath .githooks
container build -t tomi-ai-core:dev .
```

Install Gitleaks and enable the checked-in staged hook before committing; a missing scanner blocks the commit. The tests exercise rejected authentication, request schemas, model routing, untrusted provenance, fixture SSE, cancellation, Responses function boundaries, and structured answer generation. Fixture tests are distinct from provider probes in `verification/`.

`docker-compose.yml` is for local development only; production uses Azure Container Apps through Terraform. The standalone Docker build needs only this service directory. Supply `.env` at runtime and set `TOMI_HTTP_ADDR=0.0.0.0:8081` and `TOMI_GRPC_ADDR=0.0.0.0:50051` in containers. Apple Container can run the same OCI image; use a host-reachable `TOMI_HERMES_URL` when the source service runs outside its network.

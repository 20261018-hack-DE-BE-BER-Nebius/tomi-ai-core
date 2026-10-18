run:
    cargo run --bin tomi-ai-core

check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test --locked
    cargo audit
    python3 scripts/scan-secrets.py

build:
    container build -t tomi-ai-core:dev .

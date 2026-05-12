# rinha-backend-2026

Backend in Rust (Axum) for the Rinha.

## Prereqs

- Rust toolchain (optional, only if you want to run without Docker)
- Docker + Docker Compose

## Data files

This project expects a preprocessed binary dataset file:

- `test.bin` (used at runtime; mounted into containers)

To generate it (requires `references.json.gz` in the repo root):

```bash
cargo run --release --bin preprocessing
```

That will create `test.bin` in the repo root.

## Run with Docker

```bash
docker compose up --build
```

The load balancer is exposed on:

- `http://localhost:9999`

## Config

- `DB_PATH`: path to the dataset file (defaults to `test.bin`).

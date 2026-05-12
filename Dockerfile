# ESTÁGIO 1: Compilação (Builder)
FROM rust:1.95-slim-bookworm AS builder

WORKDIR /usr/src/app

# Copia manifest e código-fonte
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY resources/mcc_risk.json ./

# Compila o binário da API
#RUN cargo build --release --bin rinha-backend-2026
# IMPORTANTE: O test.bin deve estar na mesma pasta que este Dockerfile
COPY references.json.gz ./

# 4. Compila o seu projeto de verdade
RUN cargo build --release
RUN cargo run --release --bin preprocessing

# ESTÁGIO 2: Runtime (Imagem Final Mínima)
FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

# Copia apenas o necessário do estágio anterior
COPY --from=builder /usr/src/app/target/release/rinha-backend-2026 ./api
COPY --from=builder /usr/src/app/mcc_risk.json ./
COPY --from=builder /usr/src/app/test.bin ./

# Expõe a porta interna da API
EXPOSE 8080

# Comando para rodar
CMD ["./api"]
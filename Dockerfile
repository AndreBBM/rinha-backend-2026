# ESTÁGIO 1: Compilação (Builder)
FROM rust:1.95-slim-bookworm AS builder

WORKDIR /usr/src/app

# Copia manifest e código-fonte
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Dados usados em runtime
COPY resources/mcc_risk.json ./

# Compila o binário da API
RUN cargo build --release --bin rinha-backend-2026

# ESTÁGIO 2: Runtime (Imagem Final Mínima)
FROM debian:bookworm-slim

WORKDIR /app

# Copia apenas o necessário do estágio anterior
COPY --from=builder /usr/src/app/target/release/rinha-backend-2026 ./api
COPY --from=builder /usr/src/app/mcc_risk.json ./

# Expõe a porta interna da API
EXPOSE 8080

# Comando para rodar
CMD ["./api"]
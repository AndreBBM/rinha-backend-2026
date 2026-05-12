use axum::{
    body::Body,
    http::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Datelike, Timelike};
use std::collections::HashMap;
use std::sync::Arc;
use memmap2::Mmap;
use std::fs::File;
use std::io::{self, Write};

// --- CONSTANTES DE NORMALIZAÇÃO ---
const MAX_AMOUNT: f32 = 10000.0;
const MAX_INSTALLMENTS: f32 = 12.0;
const AMOUNT_VS_AVG_RATIO: f32 = 10.0;
const MAX_MINUTES: f32 = 1440.0;
const MAX_KM: f32 = 1000.0;
const MAX_TX_COUNT_24H: f32 = 20.0;
const MAX_MERCHANT_AVG_AMOUNT: f32 = 10000.0;

// --- MCC RISK MAP (exemplo fixo, idealmente carregado de mcc_risk.json) ---
fn get_mcc_risk(mcc: &str) -> f32 {
    match mcc {
        "5411" => 0.8,
        "5812" => 0.30,
        "5912" => 0.20,
        "5944" => 0.45,
        "7801" => 0.80,
        "7802" => 0.75,
        "7995" => 0.85,
        "4511" => 0.35,
        "5311" => 0.25,
        "5999" => 0.50,
        _ => 0.5, // Valor padrão para MCC desconhecido
    }
}

// --- MODELOS DE ENTRADA (API.md) ---

#[derive(Deserialize)]
struct TransactionPayload {
    id: String,
    transaction: TransactionData,
    customer: CustomerData,
    merchant: MerchantData,
    terminal: TerminalData,
    last_transaction: Option<LastTransactionData>,
}

#[derive(Deserialize)]
struct TransactionData {
    amount: f32,
    installments: u8,
    requested_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct CustomerData {
    avg_amount: f32,
    tx_count_24h: u32,
    known_merchants: Vec<String>,
}

#[derive(Deserialize)]
struct MerchantData {
    id: String,
    mcc: String,
    avg_amount: f32,
}

#[derive(Deserialize)]
struct TerminalData {
    is_online: bool,
    card_present: bool,
    km_from_home: f32,
}

#[derive(Deserialize)]
struct LastTransactionData {
    timestamp: DateTime<Utc>,
    km_from_current: f32,
}

#[derive(Serialize)]
struct FraudResponse {
    approved: bool,
    fraud_score: f32,
}

// --- LÓGICA DE VETORIZAÇÃO ---

fn clamp(val: f32) -> f32 {
    val.max(0.0).min(1.0)
}

fn vectorize(payload: &TransactionPayload) -> [f32; 14] {
    let mut v = [0.0; 14];

    // 0: amount
    v[0] = clamp(payload.transaction.amount / MAX_AMOUNT);
    
    // 1: installments
    v[1] = clamp(payload.transaction.installments as f32 / MAX_INSTALLMENTS);
    
    // 2: amount_vs_avg
    v[2] = clamp((payload.transaction.amount / payload.customer.avg_amount) / AMOUNT_VS_AVG_RATIO);
    
    // 3: hour_of_day (UTC)
    v[3] = payload.transaction.requested_at.hour() as f32 / 23.0;
    
    // 4: day_of_week (Seg=0, Dom=6)
    // weekday() do Chrono retorna 0-6 (Seg-Dom)
    v[4] = payload.transaction.requested_at.weekday().num_days_from_monday() as f32 / 6.0;

    // 5 e 6: Dependem de last_transaction
    if let Some(last) = &payload.last_transaction {
        let duration = payload.transaction.requested_at.signed_duration_since(last.timestamp);
        let minutes = duration.num_minutes().abs() as f32;
        v[5] = clamp(minutes / MAX_MINUTES);
        v[6] = clamp(last.km_from_current / MAX_KM);
    } else {
        v[5] = -1.0;
        v[6] = -1.0;
    }

    // 7: km_from_home
    v[7] = clamp(payload.terminal.km_from_home / MAX_KM);
    
    // 8: tx_count_24h
    v[8] = clamp(payload.customer.tx_count_24h as f32 / MAX_TX_COUNT_24H);
    
    // 9: is_online
    v[9] = if payload.terminal.is_online { 1.0 } else { 0.0 };
    
    // 10: card_present
    v[10] = if payload.terminal.card_present { 1.0 } else { 0.0 };
    
    // 11: unknown_merchant
    let is_known = payload.customer.known_merchants.contains(&payload.merchant.id);
    v[11] = if is_known { 0.0 } else { 1.0 };
    
    // 12: mcc_risk
    v[12] = get_mcc_risk(&payload.merchant.mcc);
    
    // 13: merchant_avg_amount
    v[13] = clamp(payload.merchant.avg_amount / MAX_MERCHANT_AVG_AMOUNT);

    v
}

fn distancia_euclidiana(v1: &[f32; 14], v2: &[f32; 14]) -> f32 {
    v1.iter().zip(v2.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}

// --- ESTADO DA APLICAÇÃO ---

struct AppState {
    // Aqui você carregará os 3 milhões de vetores do dataset
    // Para 350MB, use algo como Box<[[f32; 14]]> ou mmap
    data: Mmap,
}

// --- HANDLERS ---

async fn ready() -> &'static str {
    "OK"
}

async fn log_request(req: Request<Body>, next: Next) -> Response {
    println!("{} {}", req.method(), req.uri().path());
    next.run(req).await
}

async fn fraud_score(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(payload): Json<TransactionPayload>,
) -> Json<FraudResponse> {
    let input_vector = vectorize(&payload);
    print!("Input vector: {:?} - ", input_vector);
    io::stdout().flush().ok();
    let mut mais_proximos: [f32; 10] = [f32::MAX; 10];
    let mut distancia: f32;
    
    for i in 0..3_000_000 {
        let start = i * 57;
        let end = start + 56;
        let vec_bytes: &[u8] = &state.data[start..end];
        let legit: u8 = state.data[end];
        
        let mut vetor_comparacao = [0.0_f32; 14];
        for j in 0..14 {
            let bytes = &vec_bytes[j*4..(j+1)*4];
            vetor_comparacao[j] = f32::from_le_bytes(bytes.try_into().unwrap());
        }

        distancia = distancia_euclidiana(&input_vector, &vetor_comparacao);
        
        if distancia < mais_proximos[8] {
            for k in 0..5 {
                if distancia < mais_proximos[k * 2] {
                    // "Shift right": move os vizinhos piores para o lado para abrir espaço
                    for shift in (k + 1..5).rev() {
                        mais_proximos[shift * 2] = mais_proximos[(shift - 1) * 2];
                        mais_proximos[shift * 2 + 1] = mais_proximos[(shift - 1) * 2 + 1];
                    }
                    // Insere o novo vizinho na posição k
                    mais_proximos[k * 2] = distancia;
                    mais_proximos[k * 2 + 1] = legit as f32;
                    break;
                }
            }
        }
    }
    
    let score = (mais_proximos[1]  + mais_proximos[3] + mais_proximos[5] + mais_proximos[7] + mais_proximos[9]) / 5.0; 
    print!("Score: {}", score);
    print!(" - Vizinhos mais próximos (distância, label): ");
    for k in 0..5 {
        print!("({:.4}, {}) ", mais_proximos[k*2], mais_proximos[k*2 + 1] as u8);
    }
    io::stdout().flush().ok();

    Json(FraudResponse {
        approved: score < 0.6,
        fraud_score: score,
    })
}

#[tokio::main]
async fn main() {
    // 1. Carrega o dataset binário (3 milhões de vetores + label) usando mmap
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "test.bin".to_string());
    let file = File::open(&db_path)
        .unwrap_or_else(|e| panic!("Falha ao abrir o dataset binário em '{}': {}", db_path, e));
    let metadata = std::fs::metadata("test.bin").expect("ERRO: Arquivo test.bin não encontrado!");
    println!("Arquivo test.bin encontrado. Tamanho: {} bytes", metadata.len());
    
    // 2. Faz o mapeamento na memória (mmap)
    // Usamos unsafe porque se o arquivo for modificado externamente enquanto mapeado, é Undefined Behavior.
    // Na Rinha, o arquivo é estático, então é seguro.
    let mmap = unsafe { Mmap::map(&file).expect("Falha ao mapear o arquivo") };

    println!("Dataset mapeado com sucesso. Tamanho: {} bytes", mmap.len());

    let state = Arc::new(AppState {
        data: mmap,
    });

    let app = Router::new()
        .route("/ready", get(ready))
        .route("/fraud-score", post(fraud_score))
        .layer(middleware::from_fn(log_request))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
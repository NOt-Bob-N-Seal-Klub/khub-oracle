mod config;
mod sot;
mod connectors;
mod tui;
mod static_files;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade, Message as WsMessage}, Query, State},
    response::IntoResponse,
    routing::get,
    Router, Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::OnceLock;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use chrono::{DateTime, Utc};

use connectors::price::PriceTick;

static PRICE_BUS: OnceLock<broadcast::Sender<PriceTick>> = OnceLock::new();
pub fn price_bus() -> broadcast::Sender<PriceTick> {
    PRICE_BUS.get().unwrap().clone()
}

#[derive(Clone, Serialize)]
pub struct BlockEvent {
    pub network: String,
    pub chain_id: i32,
    pub height: i32,
    pub hash: String,
    pub parent: String,
    pub creation_time: i64,
    pub source: String,
    pub detected_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub tx: broadcast::Sender<FeedEvent>,
    pub sot: std::sync::Arc<sot::Sot>,
    pub block_tx: broadcast::Sender<BlockEvent>,
}

#[derive(Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct FeedEvent {
    pub id: uuid::Uuid,
    pub source_name: String,
    pub source_kind: String,
    pub external_id: String,
    pub title: String,
    pub snippet: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub category: String,
    pub tags: Option<Vec<String>>,
    pub published_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct FeedQuery {
    pub source: Option<String>,
    pub category: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub hours: Option<i32>,
    pub source: Option<String>,
}

#[derive(Deserialize)]
pub struct SotCheckQuery { pub url: String }

#[derive(Deserialize)]
pub struct ProxyQuery { pub path: String }

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let cfg = config::load("oracle.toml").expect("load oracle.toml");

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database.url)
        .await
        .expect("db connect");
    println!("✓ Connected to Postgres");

    let sot_data = std::sync::Arc::new(
        sot::Sot::load("sot").unwrap_or_else(|e| {
            eprintln!("[sot] failed to load sot/: {} — using empty registry", e);
            sot::Sot {
                good: sot::GoodRegistry { canonical: vec![], commands: vec![] },
                bad: sot::BadRegistry { deprecated: vec![] },
                bad_index: std::collections::HashMap::new(),
                good_urls: vec![],
            }
        })
    );
    println!("✓ SOT loaded: {} canonical, {} deprecated, {} commands",
        sot_data.good.canonical.len(), sot_data.bad.deprecated.len(), sot_data.good.commands.len());

    // ── TUI mode: run and exit, never touch the HTTP port ──
    if std::env::args().nth(1).as_deref() == Some("tui") {
        println!("✓ Starting TUI viewer (connectors not spawned — run `cargo run` in another terminal for live polling)");
        if let Err(e) = tui::run(pool).await {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // ── API mode: spawn pollers, serve HTTP ──
    connectors::seed_sources(&pool, &cfg).await;

    let (tx, _) = broadcast::channel::<FeedEvent>(512);
    let (price_tx, _) = broadcast::channel::<PriceTick>(512);
    let (block_tx, _) = broadcast::channel::<BlockEvent>(2048);
    let _ = PRICE_BUS.set(price_tx);

    connectors::spawn_all(pool.clone(), cfg.clone(), tx.clone(), block_tx.clone());

    let state = AppState { pool, tx, sot: sot_data.clone(), block_tx };
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/api/oracle/feed", get(get_feed))
        .route("/api/oracle/sources", get(get_sources))
        .route("/api/oracle/price/kda", get(get_price_kda))
        .route("/api/oracle/price/kda/history", get(get_price_history))
        .route("/api/oracle/sot/registry", get(get_sot_registry))
        .route("/api/oracle/sot/check", get(check_sot_url))
        .route("/api/oracle/chainweb/cut", get(get_chainweb_cut))
        .route("/api/oracle/chainweb/overview", get(get_chainweb_overview))
        .route("/api/oracle/chainweb/stream", get(chainweb_stream))
        .route("/api/oracle/chainweb/recent", get(get_chainweb_recent))
        .route("/api/oracle/chainweb/block/:hash", get(get_chainweb_block))
        .route("/api/oracle/chainweb/tx/:tx_hash", get(get_chainweb_tx))
        .route("/api/oracle/chainweb/transfers", get(get_chainweb_transfers))
        .route("/api/oracle/github/releases", get(get_github_releases))
        .route("/api/oracle/activity", get(get_activity))
        .route("/api/oracle/chainweb/proxy", get(chainweb_proxy))
        .route("/ws/oracle", get(ws_handler))
        .route("/ws/oracle/price", get(ws_price_handler))
        .fallback(static_files::serve)
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.server.port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("✓ KHUB Oracle running on http://{}", addr);
    println!("✓ UI at http://localhost:{}/", cfg.server.port);
    axum::serve(listener, app).await.unwrap();
}

// ── existing handlers ──
async fn get_feed(State(state): State<AppState>, Query(q): Query<FeedQuery>) -> Result<Json<Vec<FeedEvent>>, StatusCode> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let mut sql = String::from("SELECT id, source_name, source_kind, external_id, title, snippet, body, url, author, category, tags, published_at, fetched_at FROM oracle_items WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();
    if let Some(s) = &q.source { binds.push(s.clone()); sql.push_str(&format!(" AND source_name = ${}", binds.len())); }
    if let Some(c) = &q.category { binds.push(c.clone()); sql.push_str(&format!(" AND category = ${}", binds.len())); }
    sql.push_str(&format!(" ORDER BY published_at DESC LIMIT ${} OFFSET ${}", binds.len() + 1, binds.len() + 2));
    let mut query = sqlx::query_as::<_, FeedEvent>(&sql);
    for b in &binds { query = query.bind(b); }
    query = query.bind(limit).bind(offset);
    let rows = query.fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

async fn get_sources(State(state): State<AppState>) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let rows = sqlx::query("SELECT name, kind, enabled, poll_interval_secs, last_polled_at, last_error FROM oracle_sources ORDER BY name")
        .fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let json: Vec<serde_json::Value> = rows.into_iter().map(|r| serde_json::json!({
        "name": r.get::<String, _>("name"),
        "kind": r.get::<String, _>("kind"),
        "enabled": r.get::<bool, _>("enabled"),
        "poll_interval_secs": r.get::<i32, _>("poll_interval_secs"),
        "last_polled_at": r.try_get::<Option<DateTime<Utc>>, _>("last_polled_at").ok().flatten(),
        "last_error": r.try_get::<Option<String>, _>("last_error").ok().flatten(),
    })).collect();
    Ok(Json(json))
}

async fn get_price_kda(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = sqlx::query("SELECT DISTINCT ON (source) source, price_usd, market_cap_usd, volume_24h_usd, change_24h_pct, fetched_at FROM price_ticks WHERE asset = 'KDA' ORDER BY source, fetched_at DESC")
        .fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let json: Vec<serde_json::Value> = rows.into_iter().map(|r| serde_json::json!({
        "source": r.get::<String, _>("source"),
        "price_usd": r.try_get::<rust_decimal::Decimal, _>("price_usd").ok(),
        "market_cap_usd": r.try_get::<Option<rust_decimal::Decimal>, _>("market_cap_usd").ok().flatten(),
        "volume_24h_usd": r.try_get::<Option<rust_decimal::Decimal>, _>("volume_24h_usd").ok().flatten(),
        "change_24h_pct": r.try_get::<Option<rust_decimal::Decimal>, _>("change_24h_pct").ok().flatten(),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();
    Ok(Json(serde_json::json!({ "asset": "KDA", "ticks": json })))
}

async fn get_price_history(State(state): State<AppState>, Query(q): Query<HistoryQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let hours = q.hours.unwrap_or(720).clamp(1, 8760);
    let source = q.source.unwrap_or_else(|| "coingecko".into());
    let rows = sqlx::query(
        r#"SELECT date_trunc('hour', fetched_at) as ts, AVG(price_usd)::float8 as price
           FROM price_ticks
           WHERE asset = 'KDA' AND source = $1
             AND fetched_at > NOW() - make_interval(hours => $2)
           GROUP BY 1 ORDER BY 1"#
    ).bind(&source).bind(hours).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let points: Vec<serde_json::Value> = rows.into_iter().map(|r| {
        let ts: DateTime<Utc> = r.get("ts");
        let price: f64 = r.get("price");
        serde_json::json!({ "t": ts, "p": price })
    }).collect();
    Ok(Json(serde_json::json!({ "asset": "KDA", "source": source, "hours": hours, "points": points })))
}

// ── SOT + central-service handlers ──
async fn get_chainweb_cut(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT ON (network, chain_id) network, chain_id, height, hash, fetched_at
           FROM chainweb_cut ORDER BY network, chain_id, fetched_at DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let mut by_net: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
    for r in rows {
        let net: String = r.get("network");
        by_net.entry(net).or_default().push(serde_json::json!({
            "chain_id": r.get::<i32, _>("chain_id"),
            "height": r.get::<i32, _>("height"),
            "hash": r.get::<String, _>("hash"),
            "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
        }));
    }
    Ok(Json(serde_json::json!({ "networks": by_net })))
}

async fn get_github_releases(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT org, repo, tag, name, url, published_at, prerelease, author, assets,
                  LEFT(COALESCE(body, ''), 600) AS body_excerpt
           FROM github_releases
           ORDER BY published_at DESC NULLS LAST, fetched_at DESC
           LIMIT 60"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let releases: Vec<serde_json::Value> = rows.into_iter().map(|r| serde_json::json!({
        "org": r.get::<String, _>("org"),
        "repo": r.get::<String, _>("repo"),
        "tag": r.get::<String, _>("tag"),
        "name": r.get::<String, _>("name"),
        "url": r.get::<String, _>("url"),
        "published_at": r.try_get::<Option<DateTime<Utc>>, _>("published_at").ok().flatten(),
        "prerelease": r.try_get::<Option<bool>, _>("prerelease").ok().flatten().unwrap_or(false),
        "author": r.try_get::<Option<String>, _>("author").ok().flatten(),
        "assets": r.try_get::<Option<i32>, _>("assets").ok().flatten().unwrap_or(0),
        "body_excerpt": r.try_get::<Option<String>, _>("body_excerpt").ok().flatten(),
    })).collect();
    Ok(Json(serde_json::json!({ "releases": releases })))
}

async fn get_sot_registry(State(state): State<AppState>) -> Json<serde_json::Value> {
    let sot = &state.sot;
    Json(serde_json::json!({
        "source": "AI-KDA-Community/releases/sot",
        "source_url": "https://github.com/AI-KDA-Community/releases/tree/main/sot",
        "stats": sot.stats(),
        "canonical": sot.good.canonical,
        "commands": sot.good.commands,
        "deprecated": sot.bad.deprecated,
    }))
}

async fn check_sot_url(
    State(state): State<AppState>,
    Query(q): Query<SotCheckQuery>,
) -> Json<serde_json::Value> {
    let (status, deprecated) = state.sot.classify(&q.url);
    let mut out = serde_json::json!({
        "url": q.url,
        "status": status,
    });
    if let Some(d) = deprecated {
        out["matched"] = serde_json::json!({
            "label": d.label,
            "category": d.category,
            "reason": d.reason,
            "successor": d.successor,
            "archived_on": d.archived_on,
        });
        out["canonical_replacement"] = serde_json::json!(d.successor);
    }
    Json(out)
}


async fn get_chainweb_transfers(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"SELECT chain_id, block_height, tx_hash, module, sender, receiver, amount, fetched_at
           FROM chainweb_transfers ORDER BY fetched_at DESC LIMIT 100"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let transfers: Vec<serde_json::Value> = rows.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "block_height": r.get::<i32, _>("block_height"),
        "tx_hash": r.get::<String, _>("tx_hash"),
        "module": r.get::<String, _>("module"),
        "sender": r.get::<String, _>("sender"),
        "receiver": r.get::<String, _>("receiver"),
        "amount": r.get::<String, _>("amount"),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();
    Ok(Json(serde_json::json!({ "transfers": transfers })))
}



async fn get_chainweb_block(
    axum::extract::Path(hash): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let block = sqlx::query(
        r#"SELECT chain_id, height, hash, endpoint, fetched_at
           FROM chainweb_cut WHERE hash = $1 LIMIT 1"#
    ).bind(&hash).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(r) = block {
        let chain_id: i32 = r.get("chain_id");
        let height: i32 = r.get("height");
        
        let tx_count = sqlx::query_scalar::<_, i32>(
            "SELECT tx_count FROM chainweb_tx_samples WHERE chain_id = $1 AND height = $2"
        ).bind(chain_id).bind(height).fetch_optional(&state.pool).await.unwrap_or(None).unwrap_or(0);
        
        let transfers = sqlx::query(
            "SELECT module, sender, receiver, amount FROM chainweb_transfers WHERE chain_id = $1 AND block_height = $2"
        ).bind(chain_id).bind(height).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        let t_json: Vec<serde_json::Value> = transfers.into_iter().map(|t| serde_json::json!({
            "module": t.get::<String, _>("module"),
            "sender": t.get::<String, _>("sender"),
            "receiver": t.get::<String, _>("receiver"),
            "amount": t.get::<String, _>("amount"),
        })).collect();

        return Ok(Json(serde_json::json!({
            "chain_id": chain_id,
            "height": height,
            "hash": hash,
            "endpoint": r.try_get::<Option<String>, _>("endpoint").ok().flatten(),
            "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
            "tx_count": tx_count,
            "transfers": t_json,
        })));
    }
    Err(StatusCode::NOT_FOUND)
}

async fn get_chainweb_tx(
    axum::extract::Path(tx_hash): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let transfers = sqlx::query(
        "SELECT chain_id, block_height, tx_hash, module, sender, receiver, amount, fetched_at FROM chainweb_transfers WHERE tx_hash = $1"
    ).bind(&tx_hash).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if transfers.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let t_json: Vec<serde_json::Value> = transfers.into_iter().map(|t| serde_json::json!({
        "chain_id": t.get::<i32, _>("chain_id"),
        "block_height": t.get::<i32, _>("block_height"),
        "module": t.get::<String, _>("module"),
        "sender": t.get::<String, _>("sender"),
        "receiver": t.get::<String, _>("receiver"),
        "amount": t.get::<String, _>("amount"),
        "fetched_at": t.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();

    Ok(Json(serde_json::json!({
        "tx_hash": tx_hash,
        "transfers": t_json,
    })))
}

async fn chainweb_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.block_tx.subscribe();
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    return Some((Ok(Event::default().event("block").data(data)), rx));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn get_chainweb_recent(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;

    let blocks = sqlx::query(
        r#"SELECT DISTINCT ON (chain_id, height) chain_id, height, hash, endpoint, fetched_at
           FROM chainweb_cut WHERE network = 'mainnet01'
           ORDER BY chain_id, height, fetched_at DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut bj: Vec<serde_json::Value> = blocks.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "height": r.get::<i32, _>("height"),
        "hash": r.get::<String, _>("hash"),
        "endpoint": r.try_get::<Option<String>, _>("endpoint").ok().flatten(),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();
    bj.sort_by(|a, b| b["height"].as_i64().cmp(&a["height"].as_i64()));
    bj.truncate(60);

    let transfers = sqlx::query(
        r#"SELECT chain_id, block_height, tx_hash, module, sender, receiver, amount, fetched_at
           FROM chainweb_transfers ORDER BY fetched_at DESC LIMIT 60"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tj: Vec<serde_json::Value> = transfers.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "block_height": r.get::<i32, _>("block_height"),
        "tx_hash": r.get::<String, _>("tx_hash"),
        "module": r.get::<String, _>("module"),
        "sender": r.get::<String, _>("sender"),
        "receiver": r.get::<String, _>("receiver"),
        "amount": r.get::<String, _>("amount"),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();

    let stats = sqlx::query(
        r#"SELECT
             (SELECT MAX(height) FROM chainweb_cut WHERE network = 'mainnet01')::bigint AS max_height,
             (SELECT COUNT(*)::bigint FROM chainweb_cut WHERE network = 'mainnet01' AND fetched_at > NOW() - INTERVAL '1 hour') AS blocks_1h,
             (SELECT COUNT(*)::bigint FROM chainweb_transfers WHERE fetched_at > NOW() - INTERVAL '1 hour') AS transfers_1h,
             (SELECT COUNT(DISTINCT chain_id)::bigint FROM chainweb_cut WHERE network = 'mainnet01') AS active_chains"#
    ).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "blocks": bj,
        "transfers": tj,
        "max_height": stats.try_get::<Option<i64>, _>("max_height").ok().flatten(),
        "blocks_1h": stats.get::<i64, _>("blocks_1h"),
        "transfers_1h": stats.get::<i64, _>("transfers_1h"),
        "active_chains": stats.get::<i64, _>("active_chains"),
    })))
}

async fn chainweb_proxy(Query(q): Query<ProxyQuery>) -> Json<serde_json::Value> {
    if !q.path.starts_with("/chainweb/") {
        return Json(serde_json::json!({"error": "path must start with /chainweb/"}));
    }
    let url = format!("https://api.chainweb-community.org{}", q.path);
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().unwrap();
    match client.get(&url).send().await {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(j) => Json(j),
            Err(e) => Json(serde_json::json!({"error": format!("parse: {}", e)})),
        },
        Err(e) => Json(serde_json::json!({"error": format!("fetch: {}", e)})),
    }
}

// ── websockets ──
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |s| handle_socket(s, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(ev) => {
                    if let Ok(j) = serde_json::to_string(&ev) {
                        if socket.send(WsMessage::Text(j)).await.is_err() { break; }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

async fn ws_price_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_price_socket)
}

async fn handle_price_socket(mut socket: WebSocket) {
    let mut rx = price_bus().subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(t) => {
                    if let Ok(j) = serde_json::to_string(&t) {
                        if socket.send(WsMessage::Text(j)).await.is_err() { break; }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

async fn get_chainweb_overview(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;

    let info = sqlx::query(
        "SELECT node_version, package_version, block_delay_us, num_chains, latest_height, graph_json, fetched_at FROM chainweb_info LIMIT 1"
    ).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Latest per chain — with endpoint + how fresh
    let heights = sqlx::query(
        r#"SELECT DISTINCT ON (chain_id) chain_id, height, hash, endpoint, fetched_at
           FROM chainweb_cut WHERE network = 'mainnet01'
           ORDER BY chain_id, fetched_at DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Aggregate rate last 15 min
    let rate = sqlx::query(
        r#"SELECT date_trunc('minute', fetched_at) AS ts, SUM(delta)::int AS blocks
           FROM chainweb_block_rate WHERE fetched_at > NOW() - INTERVAL '15 minutes'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Per-chain rate last 10 min
    let per_chain_rate = sqlx::query(
        r#"SELECT chain_id, SUM(delta)::int AS blocks, COUNT(*)::int AS samples
           FROM chainweb_block_rate WHERE fetched_at > NOW() - INTERVAL '10 minutes'
           GROUP BY 1 ORDER BY 2 DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Health aggregate + per node
    let health = sqlx::query(
        r#"SELECT date_trunc('minute', fetched_at) AS ts,
                  AVG(latency_ms)::float8 AS avg_ms,
                  COUNT(*) FILTER (WHERE NOT ok)::int AS failures
           FROM chainweb_health WHERE fetched_at > NOW() - INTERVAL '10 minutes'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let nodes = sqlx::query(
        r#"SELECT DISTINCT ON (endpoint) endpoint, latency_ms, http_status, ok, fetched_at
           FROM chainweb_health WHERE endpoint IS NOT NULL
           ORDER BY endpoint, fetched_at DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Per-node recent stability — how many failures in last hour
    let node_failures = sqlx::query(
        r#"SELECT endpoint, COUNT(*) FILTER (WHERE NOT ok)::int AS fails, COUNT(*)::int AS total
           FROM chainweb_health WHERE fetched_at > NOW() - INTERVAL '1 hour' AND endpoint IS NOT NULL
           GROUP BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Tx samples aggregate
    let txs = sqlx::query(
        r#"SELECT date_trunc('minute', fetched_at) AS ts, SUM(tx_count)::int AS tx
           FROM chainweb_tx_samples WHERE fetched_at > NOW() - INTERVAL '30 minutes'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Recent tx samples raw
    let tx_recent = sqlx::query(
        r#"SELECT chain_id, height, tx_count, fetched_at FROM chainweb_tx_samples
           ORDER BY fetched_at DESC LIMIT 12"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Recent blocks across chains
    let recent_blocks = sqlx::query(
        r#"SELECT chain_id, height, hash, fetched_at FROM chainweb_cut
           WHERE network = 'mainnet01' ORDER BY fetched_at DESC LIMIT 20"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let info_json = info.map(|r| serde_json::json!({
        "node_version": r.get::<String, _>("node_version"),
        "package_version": r.get::<String, _>("package_version"),
        "block_delay_us": r.get::<i64, _>("block_delay_us"),
        "num_chains": r.get::<i32, _>("num_chains"),
        "latest_height": r.get::<i64, _>("latest_height"),
        "graph": r.get::<serde_json::Value, _>("graph_json"),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).unwrap_or(serde_json::Value::Null);

    let heights_json: Vec<serde_json::Value> = heights.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "height": r.get::<i32, _>("height"),
        "hash": r.get::<String, _>("hash"),
        "endpoint": r.try_get::<Option<String>, _>("endpoint").ok().flatten(),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();

    let rate_json: Vec<serde_json::Value> = rate.into_iter().map(|r| serde_json::json!({
        "t": r.get::<DateTime<Utc>, _>("ts"),
        "blocks": r.get::<i32, _>("blocks"),
    })).collect();

    let per_chain_json: Vec<serde_json::Value> = per_chain_rate.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "blocks": r.get::<i32, _>("blocks"),
        "samples": r.get::<i32, _>("samples"),
    })).collect();

    let health_json: Vec<serde_json::Value> = health.into_iter().map(|r| serde_json::json!({
        "t": r.get::<DateTime<Utc>, _>("ts"),
        "avg_ms": r.get::<f64, _>("avg_ms"),
        "failures": r.get::<i32, _>("failures"),
    })).collect();

    let nodes_json: Vec<serde_json::Value> = nodes.into_iter().map(|r| serde_json::json!({
        "endpoint": r.try_get::<Option<String>, _>("endpoint").ok().flatten().unwrap_or_default(),
        "latency_ms": r.get::<i32, _>("latency_ms"),
        "http_status": r.get::<i32, _>("http_status"),
        "ok": r.get::<bool, _>("ok"),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();

    let node_fail_json: Vec<serde_json::Value> = node_failures.into_iter().map(|r| serde_json::json!({
        "endpoint": r.get::<String, _>("endpoint"),
        "fails": r.get::<i32, _>("fails"),
        "total": r.get::<i32, _>("total"),
    })).collect();

    let tx_json: Vec<serde_json::Value> = txs.into_iter().map(|r| serde_json::json!({
        "t": r.get::<DateTime<Utc>, _>("ts"),
        "tx": r.get::<i32, _>("tx"),
    })).collect();

    let tx_recent_json: Vec<serde_json::Value> = tx_recent.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "height": r.get::<i32, _>("height"),
        "tx_count": r.get::<i32, _>("tx_count"),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();

    let blocks_json: Vec<serde_json::Value> = recent_blocks.into_iter().map(|r| serde_json::json!({
        "chain_id": r.get::<i32, _>("chain_id"),
        "height": r.get::<i32, _>("height"),
        "hash": r.get::<String, _>("hash"),
        "fetched_at": r.get::<DateTime<Utc>, _>("fetched_at"),
    })).collect();

    Ok(Json(serde_json::json!({
        "info": info_json,
        "heights": heights_json,
        "rate": rate_json,
        "per_chain_rate": per_chain_json,
        "health": health_json,
        "nodes": nodes_json,
        "node_failures": node_fail_json,
        "txs": tx_json,
        "tx_recent": tx_recent_json,
        "recent_blocks": blocks_json,
    })))
}

async fn get_activity(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;

    // chain: last poll time + recent 15-min rate + 30-day daily blocks
    let chain_last = sqlx::query("SELECT MAX(fetched_at) AS t FROM chainweb_cut")
        .fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let chain_last_t: Option<DateTime<Utc>> = chain_last.try_get("t").ok().flatten();
    let chain_alive = chain_last_t.map(|t| (Utc::now() - t).num_seconds() < 180).unwrap_or(false);

    let chain_rate = sqlx::query(
        r#"SELECT date_trunc('minute', fetched_at) AS ts, SUM(delta)::int AS blocks
           FROM chainweb_block_rate WHERE fetched_at > NOW() - INTERVAL '15 minutes'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let chain_daily = sqlx::query(
        r#"SELECT date_trunc('day', fetched_at) AS day, SUM(delta)::int AS blocks
           FROM chainweb_block_rate WHERE fetched_at > NOW() - INTERVAL '30 days'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // commits: 30-day daily + per-repo + top authors
    let commits_30d = sqlx::query(
        r#"SELECT date_trunc('day', committed_at) AS day, COUNT(*)::int AS c
           FROM github_commits WHERE committed_at > NOW() - INTERVAL '30 days'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let commits_by_repo = sqlx::query(
        r#"SELECT org || '/' || repo AS repo, COUNT(*)::int AS c, MAX(committed_at) AS last
           FROM github_commits GROUP BY 1 ORDER BY 2 DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let commits_by_author = sqlx::query(
        r#"SELECT author, COUNT(*)::int AS c
           FROM github_commits WHERE author IS NOT NULL AND author <> ''
           GROUP BY 1 ORDER BY 2 DESC LIMIT 10"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // social: 30-day daily + 30-day by source
    let social_30d = sqlx::query(
        r#"SELECT date_trunc('day', published_at) AS day, COUNT(*)::int AS posts,
                  COALESCE(AVG(sentiment), 0)::float8 AS sentiment
           FROM oracle_items
           WHERE source_kind IN ('rss','telegram','web')
             AND published_at > NOW() - INTERVAL '30 days'
           GROUP BY 1 ORDER BY 1"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let social_by_source = sqlx::query(
        r#"SELECT source_name, COUNT(*)::int AS posts, COALESCE(AVG(sentiment),0)::float8 AS sent
           FROM oracle_items
           WHERE source_kind IN ('rss','telegram','web')
             AND published_at > NOW() - INTERVAL '30 days'
           GROUP BY 1 ORDER BY 2 DESC"#
    ).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // data range: how far back does each table actually go
    let range = sqlx::query(
        r#"SELECT
             (SELECT MIN(committed_at) FROM github_commits) AS commits_first,
             (SELECT MAX(committed_at) FROM github_commits) AS commits_last,
             (SELECT MIN(published_at) FROM oracle_items WHERE source_kind IN ('rss','telegram','web')) AS social_first,
             (SELECT MAX(published_at) FROM oracle_items WHERE source_kind IN ('rss','telegram','web')) AS social_last,
             (SELECT MIN(fetched_at) FROM chainweb_block_rate) AS chain_first,
             (SELECT MAX(fetched_at) FROM chainweb_block_rate) AS chain_last"#
    ).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let chain_rate_json: Vec<serde_json::Value> = chain_rate.into_iter().map(|r| serde_json::json!({
        "t": r.get::<DateTime<Utc>, _>("ts"),
        "blocks": r.get::<i32, _>("blocks"),
    })).collect();

    let chain_daily_json: Vec<serde_json::Value> = chain_daily.into_iter().map(|r| serde_json::json!({
        "day": r.get::<DateTime<Utc>, _>("day"),
        "blocks": r.get::<i32, _>("blocks"),
    })).collect();

    let commits_json: Vec<serde_json::Value> = commits_30d.into_iter().map(|r| serde_json::json!({
        "day": r.get::<DateTime<Utc>, _>("day"),
        "commits": r.get::<i32, _>("c"),
    })).collect();

    let by_repo_json: Vec<serde_json::Value> = commits_by_repo.into_iter().map(|r| serde_json::json!({
        "repo": r.get::<String, _>("repo"),
        "commits": r.get::<i32, _>("c"),
        "last_commit": r.try_get::<Option<DateTime<Utc>>, _>("last").ok().flatten(),
    })).collect();

    let by_author_json: Vec<serde_json::Value> = commits_by_author.into_iter().map(|r| serde_json::json!({
        "author": r.get::<String, _>("author"),
        "commits": r.get::<i32, _>("c"),
    })).collect();

    let social_json: Vec<serde_json::Value> = social_30d.into_iter().map(|r| serde_json::json!({
        "day": r.get::<DateTime<Utc>, _>("day"),
        "posts": r.get::<i32, _>("posts"),
        "sentiment": r.get::<f64, _>("sentiment"),
    })).collect();

    let by_src_json: Vec<serde_json::Value> = social_by_source.into_iter().map(|r| serde_json::json!({
        "source": r.get::<String, _>("source_name"),
        "posts": r.get::<i32, _>("posts"),
        "sentiment": r.get::<f64, _>("sent"),
    })).collect();

    let range_json = serde_json::json!({
        "commits_first": range.try_get::<Option<DateTime<Utc>>, _>("commits_first").ok().flatten(),
        "commits_last":  range.try_get::<Option<DateTime<Utc>>, _>("commits_last").ok().flatten(),
        "social_first":  range.try_get::<Option<DateTime<Utc>>, _>("social_first").ok().flatten(),
        "social_last":   range.try_get::<Option<DateTime<Utc>>, _>("social_last").ok().flatten(),
        "chain_first":   range.try_get::<Option<DateTime<Utc>>, _>("chain_first").ok().flatten(),
        "chain_last":    range.try_get::<Option<DateTime<Utc>>, _>("chain_last").ok().flatten(),
    });

    Ok(Json(serde_json::json!({
        "chain": {
            "alive": chain_alive,
            "last_block_at": chain_last_t,
            "rate_15m": chain_rate_json,
            "daily_30d": chain_daily_json,
        },
        "github": {
            "commits_30d": commits_json,
            "by_repo": by_repo_json,
            "by_author": by_author_json,
        },
        "social": {
            "posts_30d": social_json,
            "by_source_30d": by_src_json,
        },
        "range": range_json,
    })))
}

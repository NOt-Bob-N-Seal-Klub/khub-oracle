use crate::config::ChainwebConfig;
use crate::connectors::log_run;
use futures::future::join_all;
use futures::StreamExt;
use crate::BlockEvent;
use tokio::sync::broadcast;
use reqwest::Client;
use sqlx::PgPool;
use std::time::Instant;
use tokio::time::{sleep, Duration};

fn default_endpoints() -> Vec<String> {
    vec!["https://api.chainweb-community.org".into()]
}

async fn stream_headers(pool: PgPool, block_tx: broadcast::Sender<BlockEvent>, network: String, endpoint: String) {
    let client = match Client::builder()
        .user_agent("khub-oracle/0.4")
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let url = format!("{}/chainweb/0.0/{}/header/updates", endpoint, network);
    loop {
        match client.get(&url).send().await {
            Ok(resp) => {
                println!("[chainweb:sse] connected to {}", url);
                let mut stream = resp.bytes_stream();
                let mut buf = String::new();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            buf.push_str(&String::from_utf8_lossy(&bytes));
                            while let Some(idx) = buf.find('\n') {
                                let line = buf[..idx].to_string();
                                buf = buf[idx+1..].to_string();
                                if line.trim().is_empty() { continue; }
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                    let chain_id = json["chainId"].as_i64().unwrap_or(-1) as i32;
                                    let height = json["height"].as_i64().unwrap_or(0) as i32;
                                    let hash = json["hash"].as_str().unwrap_or("").to_string();
                                    let parent = json["parent"].as_str().unwrap_or("").to_string();
                                    let creation_time = json["creationTime"].as_i64().unwrap_or(0);
                                    if chain_id < 0 || height <= 0 || hash.is_empty() { continue; }
                                    let _ = sqlx::query(
                                        "INSERT INTO chainweb_cut (network, endpoint, chain_id, height, hash) VALUES ($1,$2,$3,$4,$5)"
                                    ).bind(&network).bind(&endpoint).bind(chain_id).bind(height).bind(&hash).execute(&pool).await;
                                    let ev = BlockEvent {
                                        network: network.clone(),
                                        chain_id,
                                        height,
                                        hash,
                                        parent,
                                        creation_time,
                                        source: "sse".to_string(),
                                        detected_at: chrono::Utc::now(),
                                    };
                                    println!("[chainweb:sse] emit c{} h{} {}", ev.chain_id, ev.height, &ev.hash[..8.min(ev.hash.len())]);
                                    let _ = block_tx.send(ev);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            Err(e) => {
                eprintln!("[chainweb:sse] connect error: {}", e);
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn fetch_info(client: &Client, pool: &PgPool, network: &str, endpoint: &str) {
    let url = format!("{}/info", endpoint);
    if let Ok(r) = client.get(&url).send().await {
        if let Ok(j) = r.json::<serde_json::Value>().await {
            let nv = j["nodeVersion"].as_str().unwrap_or("?");
            let pv = j["nodePackageVersion"].as_str().unwrap_or("?");
            let delay = j["nodeBlockDelay"].as_i64().unwrap_or(0);
            let nchains = j["nodeNumberOfChains"].as_i64().unwrap_or(0) as i32;
            let latest = j["nodeLatestBehaviorHeight"].as_i64().unwrap_or(0);
            let graph = j["nodeGraphHistory"].clone();
            let _ = sqlx::query(
                r#"INSERT INTO chainweb_info
                   (network, endpoint, node_version, package_version, block_delay_us, num_chains, latest_height, graph_json)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                   ON CONFLICT (network, endpoint) DO UPDATE SET
                     node_version=EXCLUDED.node_version, package_version=EXCLUDED.package_version,
                     block_delay_us=EXCLUDED.block_delay_us, num_chains=EXCLUDED.num_chains,
                     latest_height=EXCLUDED.latest_height, graph_json=EXCLUDED.graph_json,
                     fetched_at=CURRENT_TIMESTAMP"#
            ).bind(network).bind(endpoint).bind(nv).bind(pv).bind(delay).bind(nchains).bind(latest).bind(graph)
             .execute(pool).await;
        }
    }
}

async fn backfill_transfers(client: &Client, pool: &PgPool, network: &str, endpoint: &str, limit: i64) -> i64 {
    // Find (chain_id, height) pairs from chainweb_cut that have no tx_sample yet,
    // most recent first — then hit /block for each to extract transfers.
    let rows: Vec<(i32, i32)> = sqlx::query_as(
        r#"SELECT c.chain_id, c.height
           FROM chainweb_cut c
           LEFT JOIN chainweb_tx_samples t
             ON t.chain_id = c.chain_id AND t.height = c.height AND t.network = c.network
           WHERE c.network = $1
             AND c.fetched_at > NOW() - INTERVAL '6 hours'
             AND t.id IS NULL
           GROUP BY c.chain_id, c.height
           ORDER BY c.height DESC
           LIMIT $2"#
    ).bind(network).bind(limit).fetch_all(pool).await.unwrap_or_default();

    let total = rows.len() as i64;
    let mut transfers_found = 0i64;
    for (chain_id, height) in rows {
        sample_tx(client, pool, network, endpoint, chain_id, height).await;
    }
    // Count how many transfers we have in the DB now for this network
    if let Ok(c) = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chainweb_transfers WHERE network = $1"
    ).bind(network).fetch_one(pool).await {
        transfers_found = c;
    }
    println!("[chainweb:backfill] scanned {} unsampled blocks → {} total transfers in DB", total, transfers_found);
    total
}


async fn sample_transfers(client: &Client, pool: &PgPool, network: &str, endpoint: &str, chain_id: i32, block_height: i32, payload_hash: &str) {
    use base64::Engine;
    if payload_hash.is_empty() { return; }
    let url = format!(
        "{}/chainweb/0.0/{}/chain/{}/payload/{}/outputs",
        endpoint, network, chain_id, payload_hash
    );
    let j = match client.get(&url).send().await {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(j) => j,
            Err(_) => return,
        },
        Err(_) => return,
    };
    let txs = match j["transactions"].as_array() {
        Some(t) => t,
        None => return,
    };
    // Each tx is [request_b64, response_b64]. Response contains the events.
    for tx in txs {
        // Try: array[1], then array last
        let resp_b64 = if let Some(arr) = tx.as_array() {
            arr.get(1).or_else(|| arr.last()).and_then(|v| v.as_str())
        } else {
            tx.as_str()
        };
        let resp_b64 = match resp_b64 {
            Some(s) => s,
            None => continue,
        };
        let resp_bytes = match base64::engine::general_purpose::STANDARD.decode(resp_b64) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let resp: serde_json::Value = match serde_json::from_slice(&resp_bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tx_hash = resp["reqKey"].as_str().unwrap_or("").to_string();
        let events = match resp["events"].as_array() {
            Some(e) => e,
            None => continue,
        };
        for ev in events {
            if ev["name"].as_str() != Some("TRANSFER") { continue; }
            let params = match ev["params"].as_array() {
                Some(p) => p,
                None => continue,
            };
            if params.len() < 3 { continue; }
            let module_name = ev["module"]["name"].as_str().unwrap_or("coin").to_string();
            let from = params[0].as_str().unwrap_or("").to_string();
            let to = params[1].as_str().unwrap_or("").to_string();
            // Amount can be Number (1.0e-6) or String
            let amt = if params[2].is_object() {
                params[2].get("decimal").and_then(|v| v.as_str()).unwrap_or("0").to_string()
            } else if let Some(st) = params[2].as_str() {
                st.to_string()
            } else if let Some(n) = params[2].as_f64() {
                n.to_string()
            } else {
                params[2].to_string()
            };
            if from.is_empty() || to.is_empty() { continue; }
            let _ = sqlx::query(
                "INSERT INTO chainweb_transfers (network, chain_id, block_height, tx_hash, module, sender, receiver, amount) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING"
            ).bind(network).bind(chain_id).bind(block_height).bind(&tx_hash).bind(&module_name).bind(&from).bind(&to).bind(&amt)
             .execute(pool).await;
        }
    }
}

async fn sample_tx(client: &Client, pool: &PgPool, network: &str, endpoint: &str, chain_id: i32, height: i32) {
    // /block?minheight=X&maxheight=X&limit=1 returns block X if it exists.
    // transactions live at items[0].payloadWithOutputs.transactions
    let url = format!(
        "{}/chainweb/0.0/{}/chain/{}/block?minheight={}&maxheight={}&limit=1",
        endpoint, network, chain_id, height, height
    );
    if let Ok(r) = client.get(&url).send().await {
        if let Ok(j) = r.json::<serde_json::Value>().await {
            if let Some(b) = j.get("items").and_then(|a| a.as_array()).and_then(|a| a.first()) {
                let h = b.get("header").and_then(|h| h.get("height"))
                    .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let payload_hash = b.get("header")
                    .and_then(|h| h.get("payloadHash"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("").to_string();
                let tx_count = b.get("payloadWithOutputs")
                    .and_then(|p| p.get("transactions"))
                    .and_then(|t| t.as_array())
                    .map(|a| a.len() as i32)
                    .unwrap_or(0);

                let _ = sqlx::query(
                    "INSERT INTO chainweb_tx_samples (network, chain_id, height, tx_count) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING"
                ).bind(network).bind(chain_id).bind(h).bind(tx_count).execute(pool).await;

                if tx_count > 0 && !payload_hash.is_empty() {
                    println!("[chainweb:tx] chain {} h{} → {} tx", chain_id, h, tx_count);
                    sample_transfers(client, pool, network, endpoint, chain_id, h, &payload_hash).await;
                }
            }
        }
    }
}




pub async fn run_loop(pool: PgPool, block_tx: broadcast::Sender<BlockEvent>, cfg: ChainwebConfig) {
    let client = Client::builder()
        .user_agent("khub-oracle/0.6")
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let endpoints = if cfg.endpoints.is_empty() { default_endpoints() } else { cfg.endpoints.clone() };
    // Spawn a background backfill to catch up on missed transfers
    {
        let c = client.clone();
        let p = pool.clone();
        let net = cfg.network.clone();
        let eps = endpoints.clone();
        tokio::spawn(async move {
            // Give the main loop 3s to land its first cut
            tokio::time::sleep(Duration::from_secs(3)).await;
            let ep = eps.first().cloned().unwrap_or_else(|| "https://api.chainweb-community.org".into());
            backfill_transfers(&c, &p, &net, &ep, 500).await;
        });
    }
    // ── Backfill: catch transfers on missed blocks ─────────────────────
    {
        let bc = client.clone();
        let bp = pool.clone();
        let bnet = cfg.network.clone();
        let bep = endpoints.first().cloned().unwrap_or_else(|| "https://api.chainweb-community.org".into());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            println!("[chainweb:backfill] starting on {}", bep);
            backfill_transfers(&bc, &bp, &bnet, &bep, 200).await;
        });
    }
    {
        let bp = pool.clone();
        let bt = block_tx.clone();
        let bnet = cfg.network.clone();
        let bep = endpoints.first().cloned().unwrap_or_else(|| "https://api.chainweb-community.org".into());
        tokio::spawn(async move { stream_headers(bp, bt, bnet, bep).await; });
    }
    let mut last_heights: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let mut tick: u64 = 0;

    loop {
        let start = Instant::now();

        // Probe ALL endpoints concurrently. Record health for each. Pick fastest.
        let probes: Vec<_> = endpoints.iter().map(|ep| {
            let ep = ep.clone();
            let net = cfg.network.clone();
            let client = client.clone();
            let pool = pool.clone();
            async move {
                let url = format!("{}/chainweb/0.0/{}/cut", ep, net);
                let t0 = Instant::now();
                match client.get(&url).send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        let body = resp.json::<serde_json::Value>().await;
                        let latency = t0.elapsed().as_millis() as i32;
                        let ok = body.is_ok() && status == 200;
                        let _ = sqlx::query(
                            "INSERT INTO chainweb_health (network, endpoint, latency_ms, http_status, ok) VALUES ($1,$2,$3,$4,$5)"
                        ).bind(&net).bind(&ep).bind(latency).bind(status as i32).bind(ok).execute(&pool).await;
                        (ep, body.ok(), latency, ok)
                    }
                    Err(_) => {
                        let latency = t0.elapsed().as_millis() as i32;
                        let _ = sqlx::query(
                            "INSERT INTO chainweb_health (network, endpoint, latency_ms, http_status, ok) VALUES ($1,$2,$3,$4,$5)"
                        ).bind(&net).bind(&ep).bind(latency).bind(0).bind(false).execute(&pool).await;
                        (ep, None, latency, false)
                    }
                }
            }
        }).collect();

        let results = join_all(probes).await;

        let mut winner: Option<(String, serde_json::Value, i32)> = None;
        for (ep, body, latency, ok) in results {
            if ok {
                if let Some(b) = body {
                    if winner.as_ref().map(|(_, _, l)| latency < *l).unwrap_or(true) {
                        winner = Some((ep, b, latency));
                    }
                }
            }
        }

        let mut count = 0i64;

        if let Some((winner_ep, winner_json, winner_latency)) = &winner {
            let mut to_sample: Vec<(i32, i32)> = Vec::new();
            if let Some(hashes) = winner_json.get("hashes").and_then(|h| h.as_object()) {
                for (cid_str, entry) in hashes {
                    let chain_id: i32 = match cid_str.parse() { Ok(n) => n, Err(_) => continue };
                    let height = entry.get("height").and_then(|h| h.as_i64()).unwrap_or(0) as i32;
                    let hash = entry.get("hash").and_then(|h| h.as_str()).unwrap_or("");
                    if hash.is_empty() { continue; }

                    let _ = sqlx::query(
                        "INSERT INTO chainweb_cut (network, endpoint, chain_id, height, hash) VALUES ($1,$2,$3,$4,$5)"
                    ).bind(&cfg.network).bind(winner_ep.as_str()).bind(chain_id).bind(height).bind(hash).execute(&pool).await;

                    let delta = last_heights.get(&chain_id).map(|&p| height - p).unwrap_or(0);
                    let _ = sqlx::query(
                        "INSERT INTO chainweb_block_rate (network, chain_id, height, delta) VALUES ($1,$2,$3,$4)"
                    ).bind(&cfg.network).bind(chain_id).bind(height).bind(delta).execute(&pool).await;

                    // Sample EVERY missed height between prev and current
                    if let Some(&prev) = last_heights.get(&chain_id) {
                        if height > prev {
                            for h in (prev + 1)..=height.min(prev + 5) {
                                to_sample.push((chain_id, h));
                            }
                        }
                    } else {
                        to_sample.push((chain_id, height));
                    }
                    last_heights.insert(chain_id, height);
                    count += 1;
                }
            }

            for (cid, h) in to_sample.iter() {
                sample_tx(&client, &pool, &cfg.network, winner_ep.as_str(), *cid, *h).await;
            }

            if tick % 3 == 0 {
                fetch_info(&client, &pool, &cfg.network, winner_ep.as_str()).await;
            }

            let short = winner_ep.replace("https://", "").split('.').next().unwrap_or("").to_string();
            println!("[chainweb:{}] via {} ({}ms) · {} chains · {}ms",
                cfg.name, short, winner_latency, count, start.elapsed().as_millis());
        } else {
            println!("[chainweb:{}] ALL {} endpoints down", cfg.name, endpoints.len());
        }

        if tick > 0 && tick % 60 == 0 {
            let ep = winner.as_ref().map(|w| w.0.clone()).unwrap_or_else(|| endpoints[0].clone());
            let c = client.clone();
            let p = pool.clone();
            let net = cfg.network.clone();
            tokio::spawn(async move {
                backfill_transfers(&c, &p, &net, &ep, 100).await;
            });
        }
        tick += 1;
        log_run(&pool, &cfg.name, if winner.is_some() { "ok" } else { "error" },
                count, if winner.is_none() { Some("all endpoints down") } else { None },
                start.elapsed().as_millis() as i64).await;
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

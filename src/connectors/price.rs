use crate::config::PriceConfig;
use crate::connectors::log_run;
use chrono::Utc;
use reqwest::Client;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

#[derive(Clone, serde::Serialize)]
pub struct PriceTick {
    pub source: String,
    pub asset: String,
    pub price_usd: f64,
    pub market_cap_usd: Option<f64>,
    pub volume_24h_usd: Option<f64>,
    pub change_24h_pct: Option<f64>,
    pub fetched_at: chrono::DateTime<Utc>,
}

pub async fn run_loop(pool: PgPool, tx: broadcast::Sender<PriceTick>, cfg: PriceConfig) {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();

    loop {
        let start = std::time::Instant::now();
        let result = match cfg.name.as_str() {
            "coingecko" => fetch_coingecko(&client).await,
            "coinpaprika" => fetch_coinpaprika(&client).await,
            "binance" => fetch_binance(&client).await,
            "coinmarketcap-kadena" => fetch_cmc_kadena(&client).await,
            _ => Err(anyhow::anyhow!("unknown price source: {}", cfg.name)),
        };

        match result {
            Ok(tick) => {
                let _ = sqlx::query(
                    "INSERT INTO price_ticks (source, asset, price_usd, market_cap_usd, volume_24h_usd, change_24h_pct) VALUES ($1,$2,$3,$4,$5,$6)"
                )
                .bind(&tick.source).bind(&tick.asset).bind(tick.price_usd)
                .bind(tick.market_cap_usd).bind(tick.volume_24h_usd).bind(tick.change_24h_pct)
                .execute(&pool).await;
                let _ = tx.send(tick.clone());
                log_run(&pool, &cfg.name, "ok", 1, None, start.elapsed().as_millis() as i64).await;
                println!("[price:{}] ${:.6}", cfg.name, tick.price_usd);
            }
            Err(e) => {
                log_run(&pool, &cfg.name, "error", 0, Some(&e.to_string()), start.elapsed().as_millis() as i64).await;
                eprintln!("[price:{}] error: {}", cfg.name, e);
            }
        }
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

async fn fetch_coingecko(client: &Client) -> anyhow::Result<PriceTick> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=kadena&vs_currencies=usd&include_market_cap=true&include_24hr_vol=true&include_24hr_change=true";
    let r: serde_json::Value = client.get(url).send().await?.json().await?;
    let k = &r["kadena"];
    Ok(PriceTick {
        source: "coingecko".into(), asset: "KDA".into(),
        price_usd: k["usd"].as_f64().unwrap_or(0.0),
        market_cap_usd: k["usd_market_cap"].as_f64(),
        volume_24h_usd: k["usd_24h_vol"].as_f64(),
        change_24h_pct: k["usd_24h_change"].as_f64(),
        fetched_at: Utc::now(),
    })
}

async fn fetch_coinpaprika(client: &Client) -> anyhow::Result<PriceTick> {
    let r: serde_json::Value = client.get("https://api.coinpaprika.com/v1/tickers/kadena-kda").send().await?.json().await?;
    let u = &r["quotes"]["USD"];
    Ok(PriceTick {
        source: "coinpaprika".into(), asset: "KDA".into(),
        price_usd: u["price"].as_f64().unwrap_or(0.0),
        market_cap_usd: u["market_cap"].as_f64(),
        volume_24h_usd: u["volume_24h"].as_f64(),
        change_24h_pct: u["percent_change_24h"].as_f64(),
        fetched_at: Utc::now(),
    })
}

async fn fetch_binance(client: &Client) -> anyhow::Result<PriceTick> {
    let r: serde_json::Value = client.get("https://api.binance.com/api/v3/ticker/24hr?symbol=KDAUSDT").send().await?.json().await?;
    Ok(PriceTick {
        source: "binance".into(), asset: "KDA".into(),
        price_usd: r["lastPrice"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
        market_cap_usd: None,
        volume_24h_usd: r["quoteVolume"].as_str().unwrap_or("0").parse().ok(),
        change_24h_pct: r["priceChangePercent"].as_str().unwrap_or("0").parse().ok(),
        fetched_at: Utc::now(),
    })
}

/// Free CMC slug endpoint — same JSON the CMC Kadena page uses
async fn fetch_cmc_kadena(client: &Client) -> anyhow::Result<PriceTick> {
    let url = "https://api.coinmarketcap.com/data-api/v3/cryptocurrency/detail?slug=kadena";
    let r: serde_json::Value = client.get(url).header("Accept", "application/json").send().await?.json().await?;
    let s = &r["data"]["statistics"];
    let price = s["price"].as_f64().ok_or_else(|| anyhow::anyhow!("CMC price missing"))?;
    Ok(PriceTick {
        source: "coinmarketcap-kadena".into(), asset: "KDA".into(),
        price_usd: price,
        market_cap_usd: s["marketCap"].as_f64(),
        volume_24h_usd: s["volume24h"].as_f64(),
        change_24h_pct: s["priceChangePercentage24h"].as_f64(),
        fetched_at: Utc::now(),
    })
}

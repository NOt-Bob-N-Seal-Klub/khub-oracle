use crate::config::WebConfig;
use crate::connectors::{hash_id, insert_item, log_run};
use crate::FeedEvent;
use reqwest::Client;
use scraper::{Html, Selector};
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

/// Synchronous — `Html` and `Selector` are NOT Send, so all parsing must
/// happen outside any await boundary. Returns owned Strings only.
fn scrape_headlines(body: &str) -> Vec<(usize, String)> {
    let doc = Html::parse_document(body);
    let sel = match Selector::parse("h2, h3") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    doc.select(&sel)
        .take(10)
        .enumerate()
        .map(|(i, el)| {
            let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            (i, text)
        })
        .collect()
}

pub async fn run_loop(pool: PgPool, tx: broadcast::Sender<FeedEvent>, cfg: WebConfig) {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .build()
        .unwrap();

    loop {
        let start = std::time::Instant::now();
        let mut count = 0i64;

        if let Ok(resp) = client.get(&cfg.url).send().await {
            if let Ok(body) = resp.text().await {
                let texts = scrape_headlines(&body);
                for (i, text) in texts {
                    if text.len() < 20 { continue; }
                    insert_item(
                        &pool, &tx, &cfg.name, "web",
                        &hash_id(&[&cfg.url, &i.to_string(), &text]),
                        &text, None, None, Some(&cfg.url),
                        None, "news", chrono::Utc::now(),
                    ).await;
                    count += 1;
                }
            }
        }

        log_run(&pool, &cfg.name, "ok", count, None, start.elapsed().as_millis() as i64).await;
        println!("[web:{}] fetched {} items", cfg.name, count);
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

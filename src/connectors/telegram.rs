use crate::config::TelegramConfig;
use crate::connectors::{hash_id, insert_item, log_run};
use crate::FeedEvent;
use reqwest::Client;
use scraper::{Html, Selector};
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

/// Synchronous — same reason as web.rs. All scraper types live and die here.
fn extract_messages(body: &str) -> Vec<(String, String, String)> {
    let doc = Html::parse_document(body);
    let msg_sel = match Selector::parse(".tgme_widget_message_wrap") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let text_sel = match Selector::parse(".tgme_widget_message_text") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let link_sel = match Selector::parse(".tgme_widget_message_date") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let time_sel = match Selector::parse(".tgme_widget_message_date time") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    doc.select(&msg_sel)
        .map(|msg| {
            let text: String = msg
                .select(&text_sel)
                .flat_map(|e| e.text())
                .collect::<Vec<_>>()
                .join(" ");
            let link = msg
                .select(&link_sel)
                .next()
                .and_then(|e| e.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let dt = msg
                .select(&time_sel)
                .next()
                .and_then(|e| e.value().attr("datetime"))
                .unwrap_or("")
                .to_string();
            (text, link, dt)
        })
        .collect()
}

pub async fn run_loop(pool: PgPool, tx: broadcast::Sender<FeedEvent>, cfg: TelegramConfig) {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; khub-oracle/0.1)")
        .build()
        .unwrap();

    loop {
        let start = std::time::Instant::now();
        let mut count = 0i64;
        let url = format!("https://t.me/s/{}", cfg.channel);

        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(body) = resp.text().await {
                let extracted = extract_messages(&body);
                for (text, link, dt) in extracted {
                    if text.trim().is_empty() { continue; }
                    let published = chrono::DateTime::parse_from_rfc3339(&dt)
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now());
                    let ext_id = if link.is_empty() {
                        hash_id(&[&text])
                    } else {
                        link.rsplit('/').next().unwrap_or(&link).to_string()
                    };
                    let snippet: String = text.chars().take(200).collect();

                    insert_item(
                        &pool, &tx, &cfg.name, "telegram",
                        &ext_id, &snippet, Some(&snippet), Some(&text), Some(&link),
                        None, "community", published,
                    ).await;
                    count += 1;
                }
            }
        }

        log_run(&pool, &cfg.name, "ok", count, None, start.elapsed().as_millis() as i64).await;
        println!("[telegram:{}] fetched {} items", cfg.name, count);
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

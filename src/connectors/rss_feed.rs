use crate::config::RssConfig;
use crate::connectors::{hash_id, insert_item, log_run};
use crate::FeedEvent;
use reqwest::Client;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

pub async fn run_loop(pool: PgPool, tx: broadcast::Sender<FeedEvent>, cfg: RssConfig) {
    let client = Client::builder().user_agent("khub-oracle/0.1").build().unwrap();

    loop {
        let start = std::time::Instant::now();
        let mut count = 0i64;

        if let Ok(resp) = client.get(&cfg.url).send().await {
            if let Ok(bytes) = resp.bytes().await {
                if let Ok(channel) = rss::Channel::read_from(&bytes[..]) {
                    for item in channel.items().iter().take(20) {
                        let title = item.title().unwrap_or("").to_string();
                        if title.is_empty() { continue; }
                        let link = item.link().unwrap_or("").to_string();
                        let description = item.description().unwrap_or("");
                        let pub_date = item.pub_date().unwrap_or("");
                        let guid = item.guid().map(|g| g.value()).unwrap_or(&link);
                        let published = chrono::DateTime::parse_from_rfc2822(pub_date)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now());

                        let snippet: String = strip_html(description).chars().take(200).collect();

                        insert_item(
                            &pool, &tx, &cfg.name, "rss",
                            &hash_id(&[&cfg.name, guid]),
                            &title, Some(&snippet), Some(description), Some(&link),
                            None, &cfg.category, published,
                        ).await;
                        count += 1;
                    }
                }
            }
        }

        log_run(&pool, &cfg.name, "ok", count, None, start.elapsed().as_millis() as i64).await;
        println!("[rss:{}] fetched {} items", cfg.name, count);
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

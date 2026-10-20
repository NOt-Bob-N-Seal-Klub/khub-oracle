use crate::config::GithubConfig;
use crate::connectors::{hash_id, insert_item, log_run};
use crate::FeedEvent;
use reqwest::Client;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

pub async fn run_loop(pool: PgPool, tx: broadcast::Sender<FeedEvent>, cfg: GithubConfig) {
    let client = Client::builder().user_agent("khub-oracle/0.1").build().unwrap();

    loop {
        let start = std::time::Instant::now();
        let mut count = 0i64;

        let repos_url = format!("https://api.github.com/orgs/{}/repos?sort=updated&per_page=10", cfg.org);
        let resp = client.get(&repos_url).send().await;

        if let Ok(r) = resp {
            if let Ok(repos) = r.json::<serde_json::Value>().await {
                if let Some(arr) = repos.as_array() {
                    for repo in arr {
                        let name = repo["name"].as_str().unwrap_or("");
                        let full_name = repo["full_name"].as_str().unwrap_or("");
                        let url = repo["html_url"].as_str().unwrap_or("");
                        let desc = repo["description"].as_str().unwrap_or("");
                        let updated = repo["updated_at"].as_str().unwrap_or("");

                        let published = chrono::DateTime::parse_from_rfc3339(updated)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now());

                        insert_item(
                            &pool, &tx, &cfg.name, "github",
                            &hash_id(&[full_name, updated]),
                            &format!("{}: {}", full_name, desc),
                            Some(desc), Some(desc), Some(url),
                            Some(full_name.split('/').next().unwrap_or("")),
                            "github", published,
                        ).await;
                        count += 1;
                    }
                }
            }
        }

        log_run(&pool, &cfg.name, "ok", count, None, start.elapsed().as_millis() as i64).await;
        println!("[github:{}] fetched {} items", cfg.name, count);
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

use crate::config::GithubReleasesConfig;
use crate::connectors::log_run;
use reqwest::Client;
use sqlx::PgPool;
use tokio::time::{sleep, Duration};

pub async fn run_loop(pool: PgPool, cfg: GithubReleasesConfig) {
    let client = Client::builder()
        .user_agent("khub-oracle/0.2")
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();

    loop {
        let start = std::time::Instant::now();
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases?per_page=10",
            cfg.org, cfg.repo
        );
        let mut count = 0i64;
        let mut err: Option<String> = None;

        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    err = Some(format!("HTTP {}", status.as_u16()));
                } else if let Ok(arr) = resp.json::<serde_json::Value>().await {
                    if let Some(list) = arr.as_array() {
                        for rel in list {
                            let tag = rel["tag_name"].as_str().unwrap_or("");
                            let html_url = rel["html_url"].as_str().unwrap_or("");
                            if tag.is_empty() || html_url.is_empty() { continue; }

                            let name = rel["name"].as_str().unwrap_or(tag);
                            let published = rel["published_at"].as_str()
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|d| d.with_timezone(&chrono::Utc));
                            let body_excerpt: String = rel["body"].as_str()
                                .unwrap_or("")
                                .chars().take(2000).collect();
                            let prerelease = rel["prerelease"].as_bool().unwrap_or(false);
                            let author = rel["author"]["login"].as_str().unwrap_or("");
                            let assets = rel["assets"].as_array()
                                .map(|a| a.len() as i32).unwrap_or(0);

                            let _ = sqlx::query(
                                r#"INSERT INTO github_releases
                                   (org, repo, tag, name, url, published_at, body, prerelease, author, assets)
                                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                                   ON CONFLICT (org, repo, tag) DO UPDATE SET
                                     name=EXCLUDED.name,
                                     body=EXCLUDED.body,
                                     prerelease=EXCLUDED.prerelease,
                                     assets=EXCLUDED.assets"#
                            )
                            .bind(&cfg.org).bind(&cfg.repo).bind(tag).bind(name).bind(html_url)
                            .bind(published).bind(&body_excerpt).bind(prerelease).bind(author).bind(assets)
                            .execute(&pool).await;
                            count += 1;
                        }
                    }
                }
            }
            Err(e) => { err = Some(e.to_string()); }
        }

        log_run(&pool, &cfg.name,
            if err.is_some() { "error" } else { "ok" },
            count, err.as_deref(), start.elapsed().as_millis() as i64).await;
        println!("[releases:{}] {} release(s){}", cfg.name, count,
            err.map(|e| format!(" — {}", e)).unwrap_or_default());
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

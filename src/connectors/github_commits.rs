use crate::config::GithubCommitsConfig;
use crate::connectors::log_run;
use reqwest::Client;
use sqlx::PgPool;
use tokio::time::{sleep, Duration};

pub async fn run_loop(pool: PgPool, cfg: GithubCommitsConfig) {
    let client = Client::builder()
        .user_agent("khub-oracle/0.4")
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap();

    loop {
        let start = std::time::Instant::now();
        let mut total_inserted = 0i64;
        let mut err: Option<String> = None;

        // Decide backfill depth: if <50 commits in DB, fetch deep (5 pages); else just latest page
        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::int8 FROM github_commits WHERE org = $1 AND repo = $2"
        ).bind(&cfg.org).bind(&cfg.repo)
         .fetch_one(&pool).await.unwrap_or(0);

        let pages = if existing < 50 { 5 } else { 1 };
        println!("[commits:{}] {} commit(s) in db — fetching {} page(s)", cfg.name, existing, pages);

        for page in 1..=pages {
            let url = format!(
                "https://api.github.com/repos/{}/{}/commits?per_page=100&page={}",
                cfg.org, cfg.repo, page
            );
            match client.get(&url)
                .header("Accept", "application/vnd.github+json")
                .send().await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status == 409 {
                        // Empty repo — not an error
                        break;
                    }
                    if !status.is_success() {
                        err = Some(format!("HTTP {}", status.as_u16()));
                        break;
                    }
                    if let Ok(arr) = resp.json::<serde_json::Value>().await {
                        if let Some(list) = arr.as_array() {
                            if list.is_empty() { break; }
                            for c in list {
                                let sha = c["sha"].as_str().unwrap_or("");
                                if sha.is_empty() { continue; }
                                let message: String = c["commit"]["message"].as_str().unwrap_or("")
                                    .lines().next().unwrap_or("")
                                    .chars().take(500).collect();
                                let author = c["commit"]["author"]["name"].as_str()
                                    .or_else(|| c["author"]["login"].as_str())
                                    .unwrap_or("");
                                let committed = c["commit"]["author"]["date"].as_str()
                                    .or_else(|| c["commit"]["committer"]["date"].as_str())
                                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                    .map(|d| d.with_timezone(&chrono::Utc));
                                if let Some(ts) = committed {
                                    let res = sqlx::query(
                                        "INSERT INTO github_commits (org, repo, sha, message, author, committed_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (org, repo, sha) DO NOTHING"
                                    )
                                    .bind(&cfg.org).bind(&cfg.repo).bind(sha).bind(&message).bind(author).bind(ts)
                                    .execute(&pool).await;
                                    if res.map(|r| r.rows_affected()).unwrap_or(0) > 0 {
                                        total_inserted += 1;
                                    }
                                }
                            }
                            // Small delay between pages to be polite to GitHub
                            if pages > 1 { sleep(Duration::from_millis(300)).await; }
                        }
                    }
                }
                Err(e) => { err = Some(e.to_string()); break; }
            }
        }

        log_run(&pool, &cfg.name,
            if err.is_some() { "error" } else { "ok" },
            total_inserted, err.as_deref(), start.elapsed().as_millis() as i64).await;
        println!("[commits:{}] +{} new commit(s){}", cfg.name, total_inserted,
            err.map(|e| format!(" — {}", e)).unwrap_or_default());
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

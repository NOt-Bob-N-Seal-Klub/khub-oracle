pub mod github;
pub mod rss_feed;
pub mod web;
pub mod telegram;
pub mod price;
pub mod chainweb;
pub mod github_releases;
pub mod github_commits;
pub mod social;

use crate::config::Config;
use crate::FeedEvent;
use chrono::Utc;
use sqlx::PgPool;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::broadcast;
use uuid::Uuid;

pub fn hash_id(parts: &[&str]) -> String {
    let mut h = DefaultHasher::new();
    for p in parts { p.hash(&mut h); }
    format!("{:x}", h.finish())
}

fn compute_sentiment(text: &str) -> f32 {
    let t = text.to_lowercase();
    let pos = ["surge","rally","bullish","gain","success","launch","win","positive","growth","boost","rise","breakout","adoption","partnership","milestone","record","high","upgrade","support","approve","listing"];
    let neg = ["crash","dump","bearish","loss","fail","hack","negative","drop","fall","decline","warning","risk","concern","exploit","vulnerability","bug","issue","lawsuit","ban","delay","suspend"];
    let mut p = 0i32; let mut n = 0i32;
    for w in &pos { if t.contains(w) { p += 1; } }
    for w in &neg { if t.contains(w) { n += 1; } }
    if p + n == 0 { 0.0 } else { (p as f32 - n as f32) / (p + n) as f32 }
}

pub async fn seed_sources(pool: &PgPool, cfg: &Config) {
    for g in &cfg.github {
        let _ = sqlx::query("INSERT INTO oracle_sources (name, kind, enabled, poll_interval_secs) VALUES ($1,'github',$2,$3) ON CONFLICT (name) DO UPDATE SET enabled=EXCLUDED.enabled")
            .bind(&g.name).bind(g.enabled).bind(g.poll_interval_secs as i32).execute(pool).await;
    }
    for w in &cfg.web {
        let _ = sqlx::query("INSERT INTO oracle_sources (name, kind, enabled, poll_interval_secs) VALUES ($1,'web',$2,$3) ON CONFLICT (name) DO UPDATE SET enabled=EXCLUDED.enabled")
            .bind(&w.name).bind(w.enabled).bind(w.poll_interval_secs as i32).execute(pool).await;
    }
    for t in &cfg.telegram {
        let _ = sqlx::query("INSERT INTO oracle_sources (name, kind, enabled, poll_interval_secs) VALUES ($1,'telegram',$2,$3) ON CONFLICT (name) DO UPDATE SET enabled=EXCLUDED.enabled")
            .bind(&t.name).bind(t.enabled).bind(t.poll_interval_secs as i32).execute(pool).await;
    }
    for r in &cfg.rss {
        let _ = sqlx::query("INSERT INTO oracle_sources (name, kind, enabled, poll_interval_secs) VALUES ($1,'rss',$2,$3) ON CONFLICT (name) DO UPDATE SET enabled=EXCLUDED.enabled")
            .bind(&r.name).bind(r.enabled).bind(r.poll_interval_secs as i32).execute(pool).await;
    }
    for p in &cfg.price {
        let _ = sqlx::query("INSERT INTO oracle_sources (name, kind, enabled, poll_interval_secs) VALUES ($1,'price',$2,$3) ON CONFLICT (name) DO UPDATE SET enabled=EXCLUDED.enabled")
            .bind(&p.name).bind(p.enabled).bind(p.poll_interval_secs as i32).execute(pool).await;
    }
}

pub async fn insert_item(
    pool: &PgPool,
    tx: &broadcast::Sender<FeedEvent>,
    source_name: &str,
    source_kind: &str,
    external_id: &str,
    title: &str,
    snippet: Option<&str>,
    body: Option<&str>,
    url: Option<&str>,
    author: Option<&str>,
    category: &str,
    published_at: chrono::DateTime<Utc>,
) {
    let tags: Vec<String> = vec![source_kind.to_string()];
    let sentiment = compute_sentiment(&format!("{} {}", title, snippet.unwrap_or("")));
    let result = sqlx::query_as::<_, FeedEvent>(
        r#"INSERT INTO oracle_items
           (source_name, source_kind, external_id, title, snippet, body, url, author, category, tags, published_at, sentiment)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           ON CONFLICT (source_name, external_id) DO NOTHING
           RETURNING id, source_name, source_kind, external_id, title, snippet, body, url, author, category, tags, published_at, fetched_at"#
    )
    .bind(source_name)
    .bind(source_kind)
    .bind(external_id)
    .bind(title)
    .bind(snippet)
    .bind(body)
    .bind(url)
    .bind(author)
    .bind(category)
    .bind(&tags)
    .bind(published_at)
    .bind(sentiment)
    .fetch_one(pool)
    .await;

    if let Ok(item) = result {
        let _ = tx.send(item);
    }
}

pub async fn log_run(pool: &PgPool, source: &str, status: &str, items: i64, error: Option<&str>, duration_ms: i64) {
    let _ = sqlx::query("INSERT INTO oracle_runs (source_name, status, items_fetched, error, duration_ms) VALUES ($1,$2,$3,$4,$5)")
        .bind(source).bind(status).bind(items as i32).bind(error).bind(duration_ms as i32)
        .execute(pool).await;
    let _ = sqlx::query("UPDATE oracle_sources SET last_polled_at = NOW(), last_error = $2 WHERE name = $1")
        .bind(source).bind(error).execute(pool).await;
}

pub fn spawn_all(pool: PgPool, cfg: Config, tx: broadcast::Sender<FeedEvent>, block_tx: broadcast::Sender<crate::BlockEvent>) {
    for g in cfg.github.clone() {
        if g.enabled {
            let (p, t, g2) = (pool.clone(), tx.clone(), g.clone());
            tokio::spawn(async move { github::run_loop(p, t, g2).await; });
        }
    }
    for r in cfg.rss.clone() {
        if r.enabled {
            let (p, t, r2) = (pool.clone(), tx.clone(), r.clone());
            tokio::spawn(async move { rss_feed::run_loop(p, t, r2).await; });
        }
    }
    for w in cfg.web.clone() {
        if w.enabled {
            let (p, t, w2) = (pool.clone(), tx.clone(), w.clone());
            tokio::spawn(async move { web::run_loop(p, t, w2).await; });
        }
    }
    for te in cfg.telegram.clone() {
        if te.enabled {
            let (p, t, te2) = (pool.clone(), tx.clone(), te.clone());
            tokio::spawn(async move { telegram::run_loop(p, t, te2).await; });
        }
    }
    for cw in cfg.chainweb.clone() {
        if cw.enabled {
            let (p, bt, cw2) = (pool.clone(), block_tx.clone(), cw.clone());
            tokio::spawn(async move { chainweb::run_loop(p, bt, cw2).await; });
        }
    }
    for gr in cfg.github_releases.clone() {
        if gr.enabled {
            let (p, gr2) = (pool.clone(), gr.clone());
            tokio::spawn(async move { github_releases::run_loop(p, gr2).await; });
        }
    }
    for so in cfg.social.clone() {
        if so.enabled {
            let (p, so2) = (pool.clone(), so.clone());
            tokio::spawn(async move { social::run_loop(p, so2).await; });
        }
    }
    for gc in cfg.github_commits.clone() {
        if gc.enabled {
            let p = pool.clone();
            tokio::spawn(async move { github_commits::run_loop(p, gc).await; });
        }
    }
    for pr in cfg.price.clone() {
        if pr.enabled {
            let (p, t) = (pool.clone(), crate::price_bus());
            tokio::spawn(async move { price::run_loop(p, t, pr).await; });
        }
    }
}

pub fn uuid_nil() -> Uuid { Uuid::nil() }

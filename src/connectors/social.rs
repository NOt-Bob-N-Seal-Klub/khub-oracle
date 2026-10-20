use crate::config::SocialConfig;
use crate::connectors::log_run;
use sqlx::PgPool;
use tokio::time::{sleep, Duration};

pub async fn run_loop(pool: PgPool, cfg: SocialConfig) {
    loop {
        let start = std::time::Instant::now();
        log_run(&pool, &cfg.name, "ok", 0, None, start.elapsed().as_millis() as i64).await;
        println!("[social:{}] idle", cfg.name);
        sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

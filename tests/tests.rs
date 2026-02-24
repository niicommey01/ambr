//! Integration tests for ambr (db layer with in-memory SQLite).

use ambr::db;
use sqlx::SqlitePool;

const MIB: f64 = 1024.0 * 1024.0;

async fn test_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    db::init_db(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn test_init_db() {
    let pool = test_pool().await;
    db::save_delta(&pool, "eth0", &1000, &2000).await.unwrap();
    let (rx, tx, total) = db::recent_totals(&pool, 60).await.unwrap();
    assert!(rx > 0.0 && tx > 0.0 && total > 0.0);
}

#[tokio::test]
async fn test_save_delta_and_recent_totals() {
    let pool = test_pool().await;
    db::save_delta(&pool, "lo", &1024, &2048).await.unwrap();
    db::save_delta(&pool, "lo", &512, &256).await.unwrap();

    let (rx, tx, total) = db::recent_totals(&pool, 60).await.unwrap();
    let expected_rx = (1024 + 512) as f64 / MIB;
    let expected_tx = (2048 + 256) as f64 / MIB;
    assert!((rx - expected_rx).abs() < 1e-6);
    assert!((tx - expected_tx).abs() < 1e-6);
    assert!((total - (expected_rx + expected_tx)).abs() < 1e-6);
}

#[tokio::test]
async fn test_recent_by_interface() {
    let pool = test_pool().await;
    db::save_delta(&pool, "eth0", &1000, &500).await.unwrap();
    db::save_delta(&pool, "wlan0", &2000, &1000).await.unwrap();

    let rows = db::recent_by_interface(&pool, 60).await.unwrap();
    assert_eq!(rows.len(), 2);
    let names: Vec<_> = rows.iter().map(|r| r.interface.as_str()).collect();
    assert!(names.contains(&"eth0"));
    assert!(names.contains(&"wlan0"));
    assert_eq!(rows[0].interface, "wlan0");
    assert_eq!(rows[1].interface, "eth0");
}

#[tokio::test]
async fn test_usage_by_day() {
    let pool = test_pool().await;
    db::save_delta(&pool, "eth0", &100_000, &50_000)
        .await
        .unwrap();
    let rows = db::usage_by_day(&pool, 10).await.unwrap();
    assert!(!rows.is_empty());
    assert!((rows[0].total_mib - (rows[0].rx_mib + rows[0].tx_mib)).abs() < 1e-6);
}

#[tokio::test]
async fn test_usage_by_hour() {
    let pool = test_pool().await;
    db::save_delta(&pool, "eth0", &2000, &1000).await.unwrap();
    let rows = db::usage_by_hour(&pool, 24).await.unwrap();
    assert!(!rows.is_empty());
}

#[tokio::test]
async fn test_usage_by_month() {
    let pool = test_pool().await;
    db::save_delta(&pool, "eth0", &5000, &3000).await.unwrap();
    let rows = db::usage_by_month(&pool, 12).await.unwrap();
    assert!(!rows.is_empty());
}

#[tokio::test]
async fn test_usage_by_week() {
    let pool = test_pool().await;
    db::save_delta(&pool, "eth0", &4000, &2000).await.unwrap();
    let rows = db::usage_by_week(&pool, 8).await.unwrap();
    assert!(!rows.is_empty());
}

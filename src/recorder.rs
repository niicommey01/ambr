use crate::db;
use std::collections::HashMap;
use std::time::Duration;
use sysinfo::Networks;

pub async fn run_recorder(
    pool: sqlx::SqlitePool,
    interval_secs: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut last: HashMap<String, (u64, u64)> = HashMap::new();
    let interval = Duration::from_secs(interval_secs);

    loop {
        tokio::time::sleep(interval).await;
        let networks = Networks::new_with_refreshed_list();

        for (name, data) in &networks {
            let rx = data.total_received();
            let tx = data.total_transmitted();
            if let Some(&(prev_rx, prev_tx)) = last.get(name.as_str()) {
                let rx_delta = rx.saturating_sub(prev_rx) as i64;
                let tx_delta = tx.saturating_sub(prev_tx) as i64;

                if rx_delta >= 0 && tx_delta >= 0 {
                    let _ = db::save_delta(&pool, name, &rx_delta, &tx_delta).await;
                }
            }
            last.insert(name.clone(), (rx, tx));
        }
    }
}

use sqlx::{FromRow, sqlite::SqlitePool};

pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS traffic (
            id INTEGER PRIMARY KEY AUTOINCREMENT, 
            interface TEXT NOT NULL, 
            rx_bytes INTEGER NOT NULL,
            tx_bytes INTEGER NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn save_delta(
    pool: &SqlitePool,
    interface: &str,
    rx_delta: &i64,
    tx_delta: &i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO traffic (interface, rx_bytes, tx_bytes) VALUES (?, ?, ?)")
        .bind(interface)
        .bind(rx_delta)
        .bind(tx_delta)
        .execute(pool)
        .await?;

    Ok(())
}

#[derive(FromRow)]
struct AggRow {
    period: String,
    rx: i64,
    tx: i64,
}

#[derive(Debug, Clone)]
pub struct PeriodRow {
    pub period: String,
    pub rx_mib: f64,
    pub tx_mib: f64,
    pub total_mib: f64,
}

const MIB: f64 = 1024.0 * 1024.0;

fn agg_to_period(r: AggRow) -> PeriodRow {
    let rx_mib = r.rx as f64 / MIB;
    let tx_mib: f64 = r.tx as f64 / MIB;
    PeriodRow {
        period: r.period,
        rx_mib,
        tx_mib,
        total_mib: rx_mib + tx_mib,
    }
}

pub async fn usage_by_hour(pool: &SqlitePool, limit: u32) -> Result<Vec<PeriodRow>, sqlx::Error> {
    let rows = sqlx::query_as(
        r#"
        SELECT
            strftime('%Y-%m-%d %H:00', timestamp) AS period,
            SUM(rx_bytes) AS rx,
            SUM(tx_bytes) AS tx
        FROM traffic
        WHERE timestamp >= datetime('now', '-7 days')
        GROUP BY period
        ORDER BY period DESC
        LIMIT ?
        "#,
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let out = rows.into_iter().map(agg_to_period).collect();

    Ok(out)
}

pub async fn usage_by_day(pool: &SqlitePool, limit: u32) -> Result<Vec<PeriodRow>, sqlx::Error> {
    let rows = sqlx::query_as(
        r#"
        SELECT
            strftime('%Y-%m-%d', timestamp) AS period,
            SUM(rx_bytes) AS rx,
            SUM(tx_bytes) AS tx
        FROM traffic
        GROUP BY period
        ORDER BY period DESC
        LIMIT ?
        "#,
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let out = rows.into_iter().map(agg_to_period).collect();
    Ok(out)
}

pub async fn usage_by_week(pool: &SqlitePool, limit: u32) -> Result<Vec<PeriodRow>, sqlx::Error> {
    let rows = sqlx::query_as(
        r#"
        SELECT
            strftime('%Y-W%W', timestamp) AS period,
            SUM(rx_bytes) AS rx,
            SUM(tx_bytes) AS tx
        FROM traffic
        GROUP BY period
        ORDER by period DESC
        LIMIT ?
        "#,
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let out = rows.into_iter().map(agg_to_period).collect();
    Ok(out)
}

pub async fn usage_by_month(pool: &SqlitePool, limit: u32) -> Result<Vec<PeriodRow>, sqlx::Error> {
    let rows = sqlx::query_as(
        r#"
        SELECT
            strftime('%Y-%m', timestamp) AS period,
            SUM(rx_bytes) AS rx,
            SUM(tx_bytes) AS tx
        FROM traffic
        GROUP BY period
        ORDER BY period DESC
        LIMIT ?
        "#,
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let out = rows.into_iter().map(agg_to_period).collect();

    Ok(out)
}

// ---- Live tab: recent usage (totals and per-interface) ----

/// Total rx/tx in MiB for the last `since_minutes` minutes.
pub async fn recent_totals(
    pool: &SqlitePool,
    since_minutes: u32,
) -> Result<(f64, f64, f64), sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
        r#"
        SELECT SUM(rx_bytes), SUM(tx_bytes)
        FROM traffic
        WHERE timestamp >= datetime('now', ?)
        "#,
    )
    .bind(format!("-{} minutes", since_minutes))
    .fetch_one(pool)
    .await?;

    let rx = row.0.unwrap_or(0) as f64 / MIB;
    let tx = row.1.unwrap_or(0) as f64 / MIB;
    Ok((rx, tx, rx + tx))
}

#[derive(Debug, Clone)]
pub struct LiveInterfaceRow {
    pub interface: String,
    pub rx_mib: f64,
    pub tx_mib: f64,
    pub total_mib: f64,
}

#[derive(FromRow)]
struct LiveAggRow {
    interface: String,
    rx: i64,
    tx: i64,
}

/// Per-interface usage in MiB for the last `since_minutes` minutes.
pub async fn recent_by_interface(
    pool: &SqlitePool,
    since_minutes: u32,
) -> Result<Vec<LiveInterfaceRow>, sqlx::Error> {
    let rows: Vec<LiveAggRow> = sqlx::query_as(
        r#"
        SELECT interface, SUM(rx_bytes) AS rx, SUM(tx_bytes) AS tx
        FROM traffic
        WHERE timestamp >= datetime('now', ?)
        GROUP BY interface
        ORDER BY (rx + tx) DESC
        "#,
    )
    .bind(format!("-{} minutes", since_minutes))
    .fetch_all(pool)
    .await?;

    let out = rows
        .into_iter()
        .map(|r| {
            let rx_mib = r.rx as f64 / MIB;
            let tx_mib = r.tx as f64 / MIB;
            LiveInterfaceRow {
                interface: r.interface,
                rx_mib,
                tx_mib,
                total_mib: rx_mib + tx_mib,
            }
        })
        .collect();
    Ok(out)
}

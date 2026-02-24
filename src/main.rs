use ambr::{db, recorder};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs},
};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// Networking-style palette: dark base, cyan (in/rx), green (out/tx), amber (total)
mod theme {
    use ratatui::style::Color;
    pub const BG: Color = Color::Rgb(0x0d, 0x11, 0x17);
    pub const BORDER: Color = Color::Rgb(0x30, 0x4d, 0x6d);
    pub const BORDER_FOCUS: Color = Color::Rgb(0x00, 0xbf, 0xd8);
    pub const TITLE: Color = Color::Rgb(0x00, 0xbf, 0xd8);
    pub const RX: Color = Color::Rgb(0x00, 0xbf, 0xd8); // download / in
    pub const TX: Color = Color::Rgb(0x00, 0xe6, 0x76); // upload / out
    pub const TOTAL: Color = Color::Rgb(0xff, 0xb7, 0x2b); // amber
    pub const HEADER: Color = Color::Rgb(0xe6, 0xed, 0xf3);
    pub const ROW_ALT: Color = Color::Rgb(0x16, 0x1b, 0x22);
    pub const HINT: Color = Color::Rgb(0x8b, 0x94, 0x9f);
}

fn default_db_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .map(|p| p.join("ambr"))
        .ok_or("could not resolve user data directory")?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("ambr.db"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db_path = default_db_path()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options).await?;
    db::init_db(&pool).await?;

    // Run recorder in background and write to db evry 10s
    let pool_rec = pool.clone();
    tokio::spawn(async move {
        let _ = recorder::run_recorder(pool_rec, 10).await;
    });

    // Run TUI in a separate thread
    let pool_tui = pool.clone();
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        run_tui(&pool_tui, rt.handle().clone())
    })
    .await;

    result.expect("TUI thread panicked")?;

    Ok(())
}

struct App {
    tab: usize,
    hourly: Vec<db::PeriodRow>,
    daily: Vec<db::PeriodRow>,
    weekly: Vec<db::PeriodRow>,
    monthly: Vec<db::PeriodRow>,
    // Live tab: last 1 min and 5 min totals (rx_mib, tx_mib, total_mib)
    live_1min: (f64, f64, f64),
    live_5min: (f64, f64, f64),
    live_by_interface: Vec<db::LiveInterfaceRow>,
}

impl App {
    fn new() -> Self {
        Self {
            tab: 0,
            hourly: Vec::new(),
            daily: Vec::new(),
            weekly: Vec::new(),
            monthly: Vec::new(),
            live_1min: (0.0, 0.0, 0.0),
            live_5min: (0.0, 0.0, 0.0),
            live_by_interface: Vec::new(),
        }
    }

    fn refresh_history(&mut self, pool: &sqlx::SqlitePool, rt: &tokio::runtime::Handle) {
        let p = pool.clone();
        if let Ok(v) = rt.block_on(async move { db::usage_by_hour(&p, 24).await }) {
            self.hourly = v;
        }
        let p = pool.clone();
        if let Ok(v) = rt.block_on(async move { db::usage_by_day(&p, 31).await }) {
            self.daily = v;
        }
        let p = pool.clone();
        if let Ok(v) = rt.block_on(async move { db::usage_by_week(&p, 12).await }) {
            self.weekly = v;
        }
        let p = pool.clone();
        if let Ok(v) = rt.block_on(async move { db::usage_by_month(&p, 12).await }) {
            self.monthly = v;
        }
    }

    fn refresh_live(&mut self, pool: &sqlx::SqlitePool, rt: &tokio::runtime::Handle) {
        let p = pool.clone();
        if let Ok((rx, tx, total)) = rt.block_on(async move { db::recent_totals(&p, 1).await }) {
            self.live_1min = (rx, tx, total);
        }
        let p = pool.clone();
        if let Ok((rx, tx, total)) = rt.block_on(async move { db::recent_totals(&p, 5).await }) {
            self.live_5min = (rx, tx, total);
        }
        let p = pool.clone();
        if let Ok(v) = rt.block_on(async move { db::recent_by_interface(&p, 1).await }) {
            self.live_by_interface = v;
        }
    }
}

fn run_tui(
    pool: &sqlx::SqlitePool,
    rt: tokio::runtime::Handle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    const LIVE_TAB_REFRESH: Duration = Duration::from_secs(1); // real-time when on Live tab
    const LIVE_BACKGROUND_REFRESH: Duration = Duration::from_secs(2); // when on other tabs

    let mut app = App::new();
    app.refresh_history(pool, &rt);
    app.refresh_live(pool, &rt);
    let mut last_live_refresh = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        // Live section (including by-interface): every 1s on Live tab, every 2s otherwise
        let live_interval = if app.tab == 0 {
            LIVE_TAB_REFRESH
        } else {
            LIVE_BACKGROUND_REFRESH
        };
        if last_live_refresh.elapsed() >= live_interval {
            app.refresh_live(pool, &rt);
            last_live_refresh = Instant::now();
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Right | KeyCode::Tab => {
                        app.tab = (app.tab + 1) % 5;
                        app.refresh_history(pool, &rt);
                    }
                    KeyCode::Left => {
                        app.tab = app.tab.checked_sub(1).unwrap_or(4);
                        app.refresh_history(pool, &rt);
                    }
                    KeyCode::Down => {}
                    KeyCode::Up => {}
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn ui(frame: &mut Frame, app: &App) {
    // Full area background
    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(theme::BG)),
        frame.area(),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let titles = [" Live ", " Hourly ", " Daily ", " Weekly ", " Monthly "];
    let tab_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " ambr ",
            Style::default()
                .fg(theme::TITLE)
                .add_modifier(Modifier::BOLD),
        ));
    let tabs = Tabs::new(titles)
        .block(tab_block)
        .style(Style::default().fg(theme::HINT))
        .highlight_style(
            Style::default()
                .fg(theme::BORDER_FOCUS)
                .add_modifier(Modifier::BOLD),
        )
        .select(app.tab);
    frame.render_widget(tabs, chunks[0]);

    let inner = chunks[1];
    match app.tab {
        0 => render_live(frame, inner, app),
        1 => render_table(frame, inner, &app.hourly, " Hourly (MiB) "),
        2 => render_table(frame, inner, &app.daily, " Daily (MiB) "),
        3 => render_table(frame, inner, &app.weekly, " Weekly (MiB) "),
        4 => render_table(frame, inner, &app.monthly, " Monthly (MiB) "),
        _ => {}
    }

    let hint = Paragraph::new(Line::from(Span::styled(
        " ← → Tab  │  q / Esc  Quit",
        Style::default().fg(theme::HINT),
    )));
    frame.render_widget(hint, chunks[2]);
}

fn render_live(frame: &mut Frame, area: Rect, app: &App) {
    let (rx1, tx1, total1) = app.live_1min;
    let (rx5, tx5, total5) = app.live_5min;

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Last 1 min  ", Style::default().fg(theme::HINT)),
            Span::styled("↓ ", Style::default().fg(theme::RX)),
            Span::styled(format!("{:.2} MiB  ", rx1), Style::default().fg(theme::RX)),
            Span::styled("↑ ", Style::default().fg(theme::TX)),
            Span::styled(format!("{:.2} MiB  ", tx1), Style::default().fg(theme::TX)),
            Span::styled("◆ ", Style::default().fg(theme::TOTAL)),
            Span::styled(
                format!("{:.2} MiB", total1),
                Style::default().fg(theme::TOTAL),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Last 5 min  ", Style::default().fg(theme::HINT)),
            Span::styled("↓ ", Style::default().fg(theme::RX)),
            Span::styled(format!("{:.2} MiB  ", rx5), Style::default().fg(theme::RX)),
            Span::styled("↑ ", Style::default().fg(theme::TX)),
            Span::styled(format!("{:.2} MiB  ", tx5), Style::default().fg(theme::TX)),
            Span::styled("◆ ", Style::default().fg(theme::TOTAL)),
            Span::styled(
                format!("{:.2} MiB", total5),
                Style::default().fg(theme::TOTAL),
            ),
        ]),
        Line::from(""),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Live traffic ",
            Style::default()
                .fg(theme::TITLE)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut y = inner.y;
    for line in &lines {
        frame.render_widget(
            Paragraph::new(line.clone()).style(Style::default().bg(theme::BG)),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
        y += 1;
    }

    let table_y = y;
    let table_height = (inner.y + inner.height).saturating_sub(table_y) as u16;
    let table_area = Rect {
        x: inner.x,
        y: table_y,
        width: inner.width,
        height: table_height,
    };
    if table_area.height >= 2 {
        let header_style = Style::default()
            .fg(theme::HEADER)
            .add_modifier(Modifier::BOLD);
        let header = Row::new(vec![
            Cell::from(Span::styled("Interface", header_style)),
            Cell::from(Span::styled(
                "↓ Rx (MiB)",
                Style::default().fg(theme::RX).add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "↑ Tx (MiB)",
                Style::default().fg(theme::TX).add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "Total (MiB)",
                Style::default()
                    .fg(theme::TOTAL)
                    .add_modifier(Modifier::BOLD),
            )),
        ]);
        let table_rows: Vec<Row> = app
            .live_by_interface
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let bg = if i % 2 == 1 {
                    theme::ROW_ALT
                } else {
                    theme::BG
                };
                Row::new(vec![
                    Cell::from(Span::styled(
                        r.interface.clone(),
                        Style::default().fg(theme::HEADER),
                    ))
                    .style(Style::default().bg(bg)),
                    Cell::from(format!("{:.2}", r.rx_mib))
                        .style(Style::default().fg(theme::RX).bg(bg)),
                    Cell::from(format!("{:.2}", r.tx_mib))
                        .style(Style::default().fg(theme::TX).bg(bg)),
                    Cell::from(format!("{:.2}", r.total_mib))
                        .style(Style::default().fg(theme::TOTAL).bg(bg)),
                ])
            })
            .collect();
        let widths = [
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
        ];
        let table_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(Span::styled(
                " By interface (last 1 min) ",
                Style::default().fg(theme::TITLE),
            ));
        let table = Table::new(table_rows, widths)
            .header(header)
            .block(table_block);
        frame.render_widget(table, table_area);
    }
}

fn render_table(frame: &mut Frame, area: Rect, rows: &[db::PeriodRow], title: &str) {
    let header_style = Style::default()
        .fg(theme::HEADER)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(Span::styled("Period", header_style)),
        Cell::from(Span::styled(
            "↓ Rx (MiB)",
            Style::default().fg(theme::RX).add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "↑ Tx (MiB)",
            Style::default().fg(theme::TX).add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Total (MiB)",
            Style::default()
                .fg(theme::TOTAL)
                .add_modifier(Modifier::BOLD),
        )),
    ]);
    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let bg = if i % 2 == 1 {
                theme::ROW_ALT
            } else {
                theme::BG
            };
            Row::new(vec![
                Cell::from(Span::styled(
                    r.period.clone(),
                    Style::default().fg(theme::HEADER),
                ))
                .style(Style::default().bg(bg)),
                Cell::from(format!("{:.2}", r.rx_mib)).style(Style::default().fg(theme::RX).bg(bg)),
                Cell::from(format!("{:.2}", r.tx_mib)).style(Style::default().fg(theme::TX).bg(bg)),
                Cell::from(format!("{:.2}", r.total_mib))
                    .style(Style::default().fg(theme::TOTAL).bg(bg)),
            ])
        })
        .collect();
    let widths = [
        Constraint::Length(22),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::TITLE)
                .add_modifier(Modifier::BOLD),
        ));
    let table = Table::new(table_rows, widths).header(header).block(block);
    frame.render_widget(table, area);
}

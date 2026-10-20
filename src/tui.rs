use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::{backend::CrosstermBackend, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs, Wrap}, Frame, Terminal};
use sqlx::{PgPool, Row as SqlxRow};
use std::io;
use std::time::Duration;

const BG: Color = Color::Rgb(13, 17, 23);
const SURFACE: Color = Color::Rgb(22, 27, 34);
const BORDER: Color = Color::Rgb(48, 54, 61);
const TEXT: Color = Color::Rgb(201, 209, 217);
const MUTED: Color = Color::Rgb(139, 148, 158);
const TEAL: Color = Color::Rgb(88, 166, 166);
const VIOLET: Color = Color::Rgb(139, 130, 160);
const GREEN: Color = Color::Rgb(63, 185, 80);
const RED: Color = Color::Rgb(248, 81, 73);

#[derive(Copy, Clone, PartialEq, Eq)]
enum Tab { Overview, Prices, Feed, Chainweb, Releases, Sources, Sot }

impl Tab {
    fn titles() -> Vec<&'static str> { vec!["Overview","Prices","Feed","Chainweb","Releases","Sources","SOT"] }
    fn index(&self) -> usize { match self { Tab::Overview=>0, Tab::Prices=>1, Tab::Feed=>2, Tab::Chainweb=>3, Tab::Releases=>4, Tab::Sources=>5, Tab::Sot=>6 } }
    fn from_index(i: usize) -> Self { match i { 1=>Tab::Prices, 2=>Tab::Feed, 3=>Tab::Chainweb, 4=>Tab::Releases, 5=>Tab::Sources, 6=>Tab::Sot, _=>Tab::Overview } }
    fn next(&self) -> Self { Tab::from_index((self.index()+1)%7) }
    fn prev(&self) -> Self { Tab::from_index((self.index()+6)%7) }
}

struct PriceRow { source: String, price_usd: f64, market_cap_usd: Option<f64>, volume_24h_usd: Option<f64>, change_24h_pct: Option<f64>, fetched_at: DateTime<Utc> }
struct FeedRow { source_name: String, source_kind: String, title: String, published_at: DateTime<Utc> }
struct ChainwebRow { network: String, chain_id: i32, height: i32 }
struct ReleaseRow { org: String, repo: String, tag: String, name: String, published_at: Option<DateTime<Utc>> }
struct SourceRow { name: String, kind: String, last_polled_at: Option<DateTime<Utc>>, last_error: Option<String> }

#[derive(Default)]
struct State {
    prices: Vec<PriceRow>, feed: Vec<FeedRow>, chainweb: Vec<ChainwebRow>,
    releases: Vec<ReleaseRow>, sources: Vec<SourceRow>,
    last_refresh: Option<DateTime<Utc>>, db_ok: bool,
}

async fn fetch_state(pool: &PgPool) -> State {
    let mut s = State::default();
    if let Ok(rows) = sqlx::query(r#"SELECT DISTINCT ON (source) source, price_usd::float8 as price_usd, market_cap_usd::float8 as market_cap_usd, volume_24h_usd::float8 as volume_24h_usd, change_24h_pct::float8 as change_24h_pct, fetched_at FROM price_ticks WHERE asset = 'KDA' ORDER BY source, fetched_at DESC"#).fetch_all(pool).await {
        for r in rows { s.prices.push(PriceRow { source: r.get("source"), price_usd: r.try_get::<f64,_>("price_usd").unwrap_or(0.0), market_cap_usd: r.try_get::<Option<f64>,_>("market_cap_usd").ok().flatten(), volume_24h_usd: r.try_get::<Option<f64>,_>("volume_24h_usd").ok().flatten(), change_24h_pct: r.try_get::<Option<f64>,_>("change_24h_pct").ok().flatten(), fetched_at: r.get("fetched_at") }); }
    }
    if let Ok(rows) = sqlx::query("SELECT source_name, source_kind, title, published_at FROM oracle_items ORDER BY published_at DESC LIMIT 30").fetch_all(pool).await {
        for r in rows { s.feed.push(FeedRow { source_name: r.get("source_name"), source_kind: r.get("source_kind"), title: r.get("title"), published_at: r.get("published_at") }); }
    }
    if let Ok(rows) = sqlx::query(r#"SELECT DISTINCT ON (network, chain_id) network, chain_id, height FROM chainweb_cut ORDER BY network, chain_id, fetched_at DESC"#).fetch_all(pool).await {
        for r in rows { s.chainweb.push(ChainwebRow { network: r.get("network"), chain_id: r.get("chain_id"), height: r.get("height") }); }
    }
    if let Ok(rows) = sqlx::query("SELECT org, repo, tag, name, published_at FROM github_releases ORDER BY published_at DESC NULLS LAST LIMIT 20").fetch_all(pool).await {
        for r in rows { s.releases.push(ReleaseRow { org: r.get("org"), repo: r.get("repo"), tag: r.get("tag"), name: r.get("name"), published_at: r.try_get::<Option<DateTime<Utc>>,_>("published_at").ok().flatten() }); }
    }
    if let Ok(rows) = sqlx::query("SELECT name, kind, last_polled_at, last_error FROM oracle_sources ORDER BY name").fetch_all(pool).await {
        for r in rows { s.sources.push(SourceRow { name: r.get("name"), kind: r.get("kind"), last_polled_at: r.try_get::<Option<DateTime<Utc>>,_>("last_polled_at").ok().flatten(), last_error: r.try_get::<Option<String>,_>("last_error").ok().flatten() }); }
    }
    s.db_ok = true;
    s.last_refresh = Some(Utc::now());
    s
}

pub async fn run(pool: PgPool) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let res = run_loop(&mut terminal, pool).await;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    res
}

async fn run_loop<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, pool: PgPool) -> Result<()> {
    let mut tab = Tab::Overview;
    let mut state = fetch_state(&pool).await;
    let mut last_refresh = std::time::Instant::now();
    loop {
        terminal.draw(|f| draw(f, tab, &state))?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Tab | KeyCode::Right => tab = tab.next(),
                        KeyCode::BackTab | KeyCode::Left => tab = tab.prev(),
                        KeyCode::Char('1') => tab = Tab::Overview, KeyCode::Char('2') => tab = Tab::Prices,
                        KeyCode::Char('3') => tab = Tab::Feed, KeyCode::Char('4') => tab = Tab::Chainweb,
                        KeyCode::Char('5') => tab = Tab::Releases, KeyCode::Char('6') => tab = Tab::Sources,
                        KeyCode::Char('7') => tab = Tab::Sot,
                        KeyCode::Char('r') => { state = fetch_state(&pool).await; last_refresh = std::time::Instant::now(); }
                        _ => {}
                    }
                }
            }
        }
        if last_refresh.elapsed() >= Duration::from_secs(2) { state = fetch_state(&pool).await; last_refresh = std::time::Instant::now(); }
    }
}

fn draw(f: &mut Frame, tab: Tab, state: &State) {
    f.render_widget(Block::default().style(Style::default().bg(BG).fg(TEXT)), f.area());
    let chunks = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)]).split(f.area());
    draw_header(f, chunks[0], state);
    draw_tabs(f, chunks[1], tab);
    match tab {
        Tab::Overview => draw_overview(f, chunks[2], state),
        Tab::Prices => draw_prices(f, chunks[2], state),
        Tab::Feed => draw_feed(f, chunks[2], state),
        Tab::Chainweb => draw_chainweb(f, chunks[2], state),
        Tab::Releases => draw_releases(f, chunks[2], state),
        Tab::Sources => draw_sources(f, chunks[2], state),
        Tab::Sot => draw_sot(f, chunks[2]),
    }
    draw_footer(f, chunks[3]);
}

fn draw_header(f: &mut Frame, area: Rect, state: &State) {
    let (dot_style, status) = if state.db_ok { (Style::default().fg(GREEN), "connected") } else { (Style::default().fg(RED), "db error") };
    let last = state.last_refresh.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_else(|| "--:--:--".into());
    let line = Line::from(vec![
        Span::styled(" ◆ ", Style::default().fg(TEAL).add_modifier(Modifier::BOLD)),
        Span::styled("KHUB Oracle", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled("  Seal Klub", Style::default().fg(MUTED)),
        Span::styled("  │  ", Style::default().fg(BORDER)),
        Span::styled(":8091", Style::default().fg(VIOLET)),
        Span::styled("  │  ", Style::default().fg(BORDER)),
        Span::styled("●", dot_style),
        Span::styled(format!(" {}", status), Style::default().fg(MUTED)),
        Span::styled(format!("   updated {}", last), Style::default().fg(MUTED)),
    ]);
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER));
    f.render_widget(Paragraph::new(line).block(block).style(Style::default().bg(BG)), area);
}

fn draw_tabs(f: &mut Frame, area: Rect, tab: Tab) {
    let titles: Vec<Line> = Tab::titles().into_iter().enumerate().map(|(i, t)| {
        let style = if i == tab.index() { Style::default().fg(TEAL).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
        Line::from(Span::styled(format!(" {} {} ", i + 1, t), style))
    }).collect();
    let tabs = Tabs::new(titles).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER)))
        .select(tab.index()).highlight_style(Style::default().fg(TEAL).add_modifier(Modifier::BOLD)).style(Style::default().bg(BG));
    f.render_widget(tabs, area);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" q", Style::default().fg(TEAL).add_modifier(Modifier::BOLD)), Span::styled(" quit   ", Style::default().fg(MUTED)),
        Span::styled("tab/←→", Style::default().fg(TEAL).add_modifier(Modifier::BOLD)), Span::styled(" switch   ", Style::default().fg(MUTED)),
        Span::styled("1-7", Style::default().fg(TEAL).add_modifier(Modifier::BOLD)), Span::styled(" tab   ", Style::default().fg(MUTED)),
        Span::styled("r", Style::default().fg(TEAL).add_modifier(Modifier::BOLD)), Span::styled(" refresh", Style::default().fg(MUTED)),
    ]);
    f.render_widget(Paragraph::new(line).style(Style::default().bg(BG)), area);
}

fn draw_overview(f: &mut Frame, area: Rect, state: &State) {
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(6), Constraint::Min(3)]).split(area);
    let primary = state.prices.iter().find(|p| p.source == "coingecko").or_else(|| state.prices.first());
    let stats = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Ratio(1,4);4]).split(rows[0]);
    let make_stat = |label: &str, value: String, sub: String, style: Style| {
        let lines = vec![
            Line::from(Span::styled(label.to_uppercase(), Style::default().fg(MUTED))),
            Line::from(Span::styled(value, Style::default().fg(TEXT).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(sub, style)),
        ];
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE).fg(TEXT))
    };
    let price_str = primary.map(|p| format!("${:.6}", p.price_usd)).unwrap_or_else(|| "—".into());
    let change = primary.and_then(|p| p.change_24h_pct).unwrap_or(0.0);
    let change_str = format!("{}{:.2}% 24h", if change >= 0.0 { "▲ +" } else { "▼ " }, change.abs());
    let change_style = if change >= 0.0 { Style::default().fg(GREEN) } else { Style::default().fg(RED) };
    f.render_widget(make_stat("KDA Price", price_str, change_str, change_style), stats[0]);
    let mc = primary.and_then(|p| p.market_cap_usd).map(|v| format!("${:.0}", v)).unwrap_or_else(|| "—".into());
    f.render_widget(make_stat("Market Cap", mc, primary.map(|p| p.source.clone()).unwrap_or_default(), Style::default().fg(MUTED)), stats[1]);
    f.render_widget(make_stat("Feed Items", state.feed.len().to_string(), "recent".into(), Style::default().fg(MUTED)), stats[2]);
    let healthy = state.sources.iter().filter(|s| s.last_error.is_none()).count();
    f.render_widget(make_stat("Pollers", format!("{}/{}", healthy, state.sources.len()), "healthy".into(), Style::default().fg(MUTED)), stats[3]);
    let body = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Ratio(1,2), Constraint::Ratio(1,2)]).split(rows[1]);
    let price_rows: Vec<Row> = state.prices.iter().map(|p| {
        let change_cell = match p.change_24h_pct {
            Some(c) => Cell::from(format!("{}{:.2}%", if c >= 0.0 {"+"} else {""}, c)).style(Style::default().fg(if c >= 0.0 { GREEN } else { RED })),
            None => Cell::from("—").style(Style::default().fg(MUTED)),
        };
        Row::new(vec![Cell::from(p.source.clone()), Cell::from(format!("${:.6}", p.price_usd)).style(Style::default().fg(TEAL)), change_cell])
    }).collect();
    let price_table = Table::new(price_rows, [Constraint::Min(12), Constraint::Length(14), Constraint::Length(10)])
        .header(Row::new(vec!["Source","Price","24h Change"]).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .block(Block::default().title(" Prices ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(price_table, body[0]);
    let feed_lines: Vec<Line> = state.feed.iter().take(15).map(|fr| Line::from(vec![
        Span::styled("◈ ", Style::default().fg(TEAL)),
        Span::styled(fr.title.chars().take(60).collect::<String>(), Style::default().fg(TEXT)),
        Span::styled(format!("  [{}]", fr.source_name), Style::default().fg(MUTED)),
    ])).collect();
    let feed = Paragraph::new(if feed_lines.is_empty() { vec![Line::from(Span::styled("no items yet", Style::default().fg(MUTED)))] } else { feed_lines })
        .block(Block::default().title(" Latest Feed ").borders(Borders::ALL).border_style(Style::default().fg(BORDER)))
        .style(Style::default().bg(SURFACE)).wrap(Wrap { trim: true });
    f.render_widget(feed, body[1]);
}

fn draw_prices(f: &mut Frame, area: Rect, state: &State) {
    let rows: Vec<Row> = state.prices.iter().map(|p| {
        let change_cell = match p.change_24h_pct {
            Some(c) => Cell::from(format!("{}{:.2}%", if c >= 0.0 {"+"} else {""}, c)).style(Style::default().fg(if c >= 0.0 { GREEN } else { RED })),
            None => Cell::from("—").style(Style::default().fg(MUTED)),
        };
        Row::new(vec![
            Cell::from(p.source.clone()), Cell::from(format!("${:.6}", p.price_usd)).style(Style::default().fg(TEAL)),
            Cell::from(p.market_cap_usd.map(|v| format!("${:.0}", v)).unwrap_or_else(|| "—".into())),
            Cell::from(p.volume_24h_usd.map(|v| format!("${:.0}", v)).unwrap_or_else(|| "—".into())),
            change_cell, Cell::from(p.fetched_at.format("%H:%M:%S").to_string()).style(Style::default().fg(MUTED)),
        ])
    }).collect();
    let table = Table::new(rows, [Constraint::Min(14), Constraint::Length(14), Constraint::Length(16), Constraint::Length(16), Constraint::Length(10), Constraint::Length(10)])
        .header(Row::new(vec!["Source","Price","Market Cap","Volume 24h","24h","Updated"]).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .block(Block::default().title(" Price Sources ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(table, area);
}

fn draw_feed(f: &mut Frame, area: Rect, state: &State) {
    let rows: Vec<Row> = state.feed.iter().map(|fr| Row::new(vec![
        Cell::from(format!("[{}]", fr.source_kind)).style(Style::default().fg(TEAL)),
        Cell::from(fr.source_name.clone()).style(Style::default().fg(MUTED)),
        Cell::from(fr.title.chars().take(80).collect::<String>()).style(Style::default().fg(TEXT)),
    ])).collect();
    let table = Table::new(rows, [Constraint::Length(12), Constraint::Length(22), Constraint::Min(30)])
        .header(Row::new(vec!["Kind","Source","Title"]).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .block(Block::default().title(" Oracle Feed ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(table, area);
}

fn draw_chainweb(f: &mut Frame, area: Rect, state: &State) {
    let mut by_net: std::collections::BTreeMap<String, Vec<&ChainwebRow>> = std::collections::BTreeMap::new();
    for c in &state.chainweb { by_net.entry(c.network.clone()).or_default().push(c); }
    let mut lines: Vec<Line> = Vec::new();
    for (net, chains) in &by_net {
        lines.push(Line::from(Span::styled(format!(" {} — {} chains ", net, chains.len()), Style::default().fg(TEAL).add_modifier(Modifier::BOLD))));
        for chunk in chains.chunks(4) {
            let row: Vec<Span> = chunk.iter().flat_map(|c| vec![
                Span::styled(format!(" Chain {:>2}: ", c.chain_id), Style::default().fg(MUTED)),
                Span::styled(format!("{:>8}   ", c.height), Style::default().fg(TEXT)),
            ]).collect();
            lines.push(Line::from(row));
        }
        lines.push(Line::from(""));
    }
    if lines.is_empty() { lines.push(Line::from(Span::styled("no chainweb data collected yet", Style::default().fg(MUTED)))); }
    let para = Paragraph::new(lines).block(Block::default().title(" Chainweb Cut ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(para, area);
}

fn draw_releases(f: &mut Frame, area: Rect, state: &State) {
    let rows: Vec<Row> = state.releases.iter().map(|r| Row::new(vec![
        Cell::from(format!("{}/{}", r.org, r.repo)).style(Style::default().fg(MUTED)),
        Cell::from(r.tag.clone()).style(Style::default().fg(VIOLET)),
        Cell::from(r.name.chars().take(50).collect::<String>()),
        Cell::from(r.published_at.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".into())).style(Style::default().fg(MUTED)),
    ])).collect();
    let table = Table::new(rows, [Constraint::Min(20), Constraint::Length(14), Constraint::Min(20), Constraint::Length(12)])
        .header(Row::new(vec!["Repo","Tag","Name","Published"]).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .block(Block::default().title(" GitHub Releases — Canonical Sources ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(table, area);
}

fn draw_sources(f: &mut Frame, area: Rect, state: &State) {
    let rows: Vec<Row> = state.sources.iter().map(|s| {
        let error_cell = match &s.last_error { Some(e) => Cell::from(e.chars().take(40).collect::<String>()).style(Style::default().fg(RED)), None => Cell::from("ok").style(Style::default().fg(GREEN)) };
        let polled = s.last_polled_at.map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_else(|| "—".into());
        Row::new(vec![Cell::from(s.name.clone()), Cell::from(s.kind.clone()).style(Style::default().fg(TEAL)), Cell::from(polled).style(Style::default().fg(MUTED)), error_cell])
    }).collect();
    let table = Table::new(rows, [Constraint::Min(20), Constraint::Length(12), Constraint::Length(10), Constraint::Min(20)])
        .header(Row::new(vec!["Source","Kind","Last Polled","Last Error"]).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .block(Block::default().title(" Poller Health ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(table, area);
}

fn draw_sot(f: &mut Frame, area: Rect) {
    let rows = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Ratio(1,2), Constraint::Ratio(1,2)]).split(area);
    let canonical = vec![("Org","kda-community"),("API","api.chainweb-community.org"),("Testnet","testnet06"),("IDE","ide.forkme.fun"),("Docs","kda-chain.org/docs/"),("kadena.js","kadena-community/kadena.js")];
    let deprecated = vec![("API","api.chainweb-community.org"),("API","api.chainweb-community.org"),("Docs","kda-chain.org/docs"),("Docs","kda-chain.org/docs"),("Testnet","testnet06"),("Testnet","testnet06")];
    let build_lines = |items: &[(&str, &str)], color: Color| -> Vec<Line> {
        items.iter().map(|(k, v)| Line::from(vec![Span::styled(format!(" {:<10} ", k), Style::default().fg(MUTED)), Span::styled(v.to_string(), Style::default().fg(color))])).collect()
    };
    let left = Paragraph::new(build_lines(&canonical, TEAL)).block(Block::default().title(" Canonical ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    let right = Paragraph::new(build_lines(&deprecated, RED)).block(Block::default().title(" Deprecated ").borders(Borders::ALL).border_style(Style::default().fg(BORDER))).style(Style::default().bg(SURFACE));
    f.render_widget(left, rows[0]);
    f.render_widget(right, rows[1]);
}

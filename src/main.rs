//! Binary: load the price CSV, run the 20-day momentum backtest, print the
//! three headline metrics, and emit a shareable equity-curve report
//! (`report/equity.svg` + `report/index.html`).
//!
//! Usage: `cargo run --release` (reads `data/spy.csv`, lookback = 20).

use momentum_backtest::{
    equity_curve, metrics, momentum_positions, simple_returns, strategy_returns, Metrics,
    TRADING_DAYS,
};
use std::fs;
use std::path::Path;

const LOOKBACK: usize = 20;
const DATA_PATH: &str = "data/spy.csv";
const REPORT_DIR: &str = "report";

fn main() {
    let (dates, prices) = load_csv(DATA_PATH).unwrap_or_else(|e| {
        eprintln!("failed to read {DATA_PATH}: {e}");
        std::process::exit(1);
    });
    assert!(
        prices.len() >= 250,
        "need >= 250 trading days, got {}",
        prices.len()
    );

    let returns = simple_returns(&prices);
    let positions = momentum_positions(&prices, LOOKBACK);
    let strat_returns = strategy_returns(&returns, &positions);

    let strat_eq = equity_curve(&strat_returns);
    let bh_eq = equity_curve(&returns);

    let strat_m = metrics(&strat_returns, TRADING_DAYS);
    let bh_m = metrics(&returns, TRADING_DAYS);

    print_report(&dates, &prices, &strat_m, &bh_m, &positions);

    // Equity curves are aligned to returns (length n-1); the matching dates are
    // dates[1..] (each equity point is "after" that day's return).
    let curve_dates = &dates[1..];
    let svg = render_svg(curve_dates, &strat_eq[1..], &bh_eq[1..]);
    let html = render_html(&dates, &strat_m, &bh_m, &svg);

    fs::create_dir_all(REPORT_DIR).expect("create report dir");
    fs::write(format!("{REPORT_DIR}/equity.svg"), &svg).expect("write svg");
    fs::write(format!("{REPORT_DIR}/index.html"), &html).expect("write html");
    println!("\nWrote {REPORT_DIR}/equity.svg and {REPORT_DIR}/index.html");
}

/// Parse a `date,price` CSV with a one-line header.
fn load_csv(path: &str) -> Result<(Vec<String>, Vec<f64>), String> {
    let text = fs::read_to_string(Path::new(path)).map_err(|e| e.to_string())?;
    let mut dates = Vec::new();
    let mut prices = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // header / blank
        }
        let mut it = line.split(',');
        let date = it.next().ok_or("missing date")?.trim().to_string();
        let price: f64 = it
            .next()
            .ok_or("missing price")?
            .trim()
            .parse()
            .map_err(|_| format!("bad price on line {}", i + 1))?;
        dates.push(date);
        prices.push(price);
    }
    Ok((dates, prices))
}

fn pct(x: f64) -> String {
    format!("{:+.2}%", x * 100.0)
}

fn print_report(
    dates: &[String],
    prices: &[f64],
    strat: &Metrics,
    bh: &Metrics,
    positions: &[f64],
) {
    let days_long = positions.iter().filter(|&&p| p > 0.0).count();
    let exposure = days_long as f64 / positions.len() as f64;
    println!("=== 20-day momentum long/flat backtest (SPY) ===");
    println!(
        "Period   : {} -> {}  ({} trading days)",
        dates.first().unwrap(),
        dates.last().unwrap(),
        prices.len()
    );
    println!(
        "Exposure : {:.1}% of days long ({} / {})",
        exposure * 100.0,
        days_long,
        positions.len()
    );
    println!();
    println!("{:<22}{:>14}{:>16}", "Metric", "Momentum", "Buy & Hold");
    println!("{:-<52}", "");
    println!(
        "{:<22}{:>14}{:>16}",
        "CAGR",
        pct(strat.cagr),
        pct(bh.cagr)
    );
    println!(
        "{:<22}{:>14}{:>16}",
        "Annualized Sharpe",
        format!("{:.3}", strat.sharpe),
        format!("{:.3}", bh.sharpe)
    );
    println!(
        "{:<22}{:>14}{:>16}",
        "Max Drawdown",
        pct(strat.max_drawdown),
        pct(bh.max_drawdown)
    );
}

/// Build a self-contained SVG line chart of the two equity curves.
fn render_svg(dates: &[String], strat: &[f64], bh: &[f64]) -> String {
    const W: f64 = 960.0;
    const H: f64 = 460.0;
    const ML: f64 = 64.0;
    const MR: f64 = 184.0;
    const MT: f64 = 50.0;
    const MB: f64 = 46.0;
    let pw = W - ML - MR;
    let ph = H - MT - MB;

    let n = strat.len();
    let ymin = strat
        .iter()
        .chain(bh.iter())
        .cloned()
        .fold(f64::INFINITY, f64::min)
        .min(1.0);
    let ymax = strat
        .iter()
        .chain(bh.iter())
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = (ymax - ymin) * 0.06 + 1e-9;
    let (lo, hi) = (ymin - pad, ymax + pad);

    let sx = |i: usize| ML + pw * (i as f64) / ((n - 1).max(1) as f64);
    let sy = |v: f64| MT + ph * (1.0 - (v - lo) / (hi - lo));

    let path = |series: &[f64]| -> String {
        let mut d = String::new();
        for (i, &v) in series.iter().enumerate() {
            d.push_str(if i == 0 { "M" } else { "L" });
            d.push_str(&format!("{:.2} {:.2} ", sx(i), sy(v)));
        }
        d
    };

    // Horizontal gridlines + y labels (equity multiples).
    let mut grid = String::new();
    let ticks = 5;
    for t in 0..=ticks {
        let v = lo + (hi - lo) * (t as f64) / (ticks as f64);
        let y = sy(v);
        grid.push_str(&format!(
            "<line x1='{ML:.1}' y1='{y:.1}' x2='{:.1}' y2='{y:.1}' stroke='#e6e8ec' stroke-width='1'/>",
            ML + pw
        ));
        grid.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-size='12' fill='#6b7280' text-anchor='end'>{:.2}x</text>",
            ML - 8.0,
            y + 4.0,
            v
        ));
    }

    // X axis date labels (start, mid, end).
    let mut xlabels = String::new();
    for &i in &[0usize, n / 2, n - 1] {
        let x = sx(i);
        let anchor = if i == 0 {
            "start"
        } else if i == n - 1 {
            "end"
        } else {
            "middle"
        };
        xlabels.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-size='12' fill='#6b7280' text-anchor='{}'>{}</text>",
            x,
            MT + ph + 28.0,
            anchor,
            dates.get(i).map(String::as_str).unwrap_or("")
        ));
    }

    let strat_end = *strat.last().unwrap();
    let bh_end = *bh.last().unwrap();

    format!(
        r#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {W} {H}' font-family='-apple-system,Segoe UI,Roboto,sans-serif'>
<rect width='{W}' height='{H}' fill='#ffffff'/>
<text x='{ML}' y='28' font-size='18' font-weight='700' fill='#111827'>SPY — 20-day Momentum vs Buy &amp; Hold (growth of $1)</text>
{grid}
{xlabels}
<path d='{strat_path}' fill='none' stroke='#2563eb' stroke-width='2.4'/>
<path d='{bh_path}' fill='none' stroke='#9ca3af' stroke-width='2' stroke-dasharray='5 4'/>
<circle cx='{sx_end:.1}' cy='{sy_strat:.1}' r='3.5' fill='#2563eb'/>
<circle cx='{sx_end:.1}' cy='{sy_bh:.1}' r='3.5' fill='#9ca3af'/>
<rect x='{lx:.1}' y='{MT}' width='168' height='62' rx='8' fill='#f9fafb' stroke='#e5e7eb'/>
<line x1='{lx2:.1}' y1='{ly1:.1}' x2='{lx3:.1}' y2='{ly1:.1}' stroke='#2563eb' stroke-width='2.4'/>
<text x='{ltx:.1}' y='{lty1:.1}' font-size='12.5' fill='#111827'>Momentum {strat_end:.2}x</text>
<line x1='{lx2:.1}' y1='{ly2:.1}' x2='{lx3:.1}' y2='{ly2:.1}' stroke='#9ca3af' stroke-width='2' stroke-dasharray='5 4'/>
<text x='{ltx:.1}' y='{lty2:.1}' font-size='12.5' fill='#111827'>Buy &amp; Hold {bh_end:.2}x</text>
</svg>
"#,
        W = W,
        H = H,
        ML = ML,
        MT = MT,
        grid = grid,
        xlabels = xlabels,
        strat_path = path(strat),
        bh_path = path(bh),
        sx_end = sx(n - 1),
        sy_strat = sy(strat_end),
        sy_bh = sy(bh_end),
        lx = ML + pw + 6.0,
        lx2 = ML + pw + 16.0,
        lx3 = ML + pw + 40.0,
        ltx = ML + pw + 46.0,
        ly1 = MT + 22.0,
        ly2 = MT + 44.0,
        lty1 = MT + 26.0,
        lty2 = MT + 48.0,
        strat_end = strat_end,
        bh_end = bh_end,
    )
}

/// Self-contained HTML report embedding the SVG and the metrics table.
fn render_html(dates: &[String], strat: &Metrics, bh: &Metrics, svg: &str) -> String {
    let row = |name: &str, a: String, b: String| {
        format!("<tr><td>{name}</td><td class='num'>{a}</td><td class='num muted'>{b}</td></tr>")
    };
    let table = format!(
        "{}{}{}",
        row("CAGR", pct(strat.cagr), pct(bh.cagr)),
        row(
            "Annualized Sharpe (√252)",
            format!("{:.3}", strat.sharpe),
            format!("{:.3}", bh.sharpe)
        ),
        row("Max Drawdown", pct(strat.max_drawdown), pct(bh.max_drawdown)),
    );

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>SPY 20-day Momentum Backtest</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ font-family: -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif;
    margin: 0; background: #f3f4f6; color: #111827; }}
  .wrap {{ max-width: 980px; margin: 0 auto; padding: 32px 20px 64px; }}
  h1 {{ font-size: 26px; margin: 0 0 4px; }}
  .sub {{ color: #6b7280; margin: 0 0 24px; font-size: 14px; }}
  .card {{ background: #fff; border: 1px solid #e5e7eb; border-radius: 14px;
    padding: 18px; box-shadow: 0 1px 2px rgba(0,0,0,.04); margin-bottom: 22px; }}
  svg {{ width: 100%; height: auto; display: block; }}
  table {{ border-collapse: collapse; width: 100%; font-size: 15px; }}
  th, td {{ text-align: left; padding: 10px 12px; border-bottom: 1px solid #eef0f3; }}
  th {{ font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: #6b7280; }}
  td.num {{ text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; }}
  td.muted {{ color: #6b7280; font-weight: 500; }}
  .meta {{ font-size: 13px; color: #6b7280; line-height: 1.6; }}
  code {{ background: #f3f4f6; padding: 1px 5px; border-radius: 5px; }}
  footer {{ color: #9ca3af; font-size: 12px; margin-top: 18px; }}
</style>
</head>
<body>
<div class="wrap">
  <h1>SPY — 20-day Momentum (long/flat) Backtest</h1>
  <p class="sub">Period {start} → {end} · {ndays} trading days · daily adjusted close</p>

  <div class="card">{svg}</div>

  <div class="card">
    <table>
      <thead><tr><th>Metric</th><th style="text-align:right">Momentum</th><th style="text-align:right">Buy &amp; Hold</th></tr></thead>
      <tbody>{table}</tbody>
    </table>
  </div>

  <div class="card meta">
    <strong>Strategy.</strong> Each day, go <em>long</em> SPY if today's close exceeds the close 20
    trading days ago, otherwise stay <em>flat</em> (cash, 0% return). The position decided at a day's
    close earns the <em>next</em> day's return — no look-ahead. Sharpe is annualized with √252 and a
    0% risk-free rate; CAGR and max drawdown are computed from the compounded equity curve.<br><br>
    <strong>Reproduce.</strong> <code>cargo test</code> verifies the backtest math; <code>cargo run --release</code>
    regenerates this page from <code>data/spy.csv</code>.
    <footer>Generated by the <code>momentum-backtest</code> Rust crate. Educational use only — not investment advice.</footer>
  </div>
</div>
</body>
</html>
"#,
        start = dates.first().cloned().unwrap_or_default(),
        end = dates.last().cloned().unwrap_or_default(),
        ndays = dates.len(),
        svg = svg,
        table = table,
    )
}

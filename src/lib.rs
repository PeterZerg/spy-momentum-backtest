//! Pure, unit-tested backtest math for a long/flat momentum strategy.
//!
//! All functions here are pure (no I/O, no globals) so they can be exercised
//! directly by `cargo test`. Index/time alignment is chosen to be strictly
//! free of look-ahead bias: a signal computed at the close of day `j` only ever
//! governs the return *earned over day `j+1`*.

/// Number of trading days per year, used to annualize returns.
pub const TRADING_DAYS: f64 = 252.0;

/// Simple daily returns: `r[i] = price[i+1] / price[i] - 1`.
///
/// Output length is `prices.len() - 1` (or empty if fewer than 2 prices).
/// `returns[j]` is the return *earned over day `j+1`*.
pub fn simple_returns(prices: &[f64]) -> Vec<f64> {
    if prices.len() < 2 {
        return Vec::new();
    }
    prices
        .windows(2)
        .map(|w| w[1] / w[0] - 1.0)
        .collect()
}

/// Momentum positions aligned to [`simple_returns`].
///
/// `positions[j]` is the position (1.0 = long, 0.0 = flat) held over day `j+1`,
/// decided at the close of day `j`: long iff `price[j] > price[j - lookback]`.
/// Because the decision uses only data available at the close of day `j` and is
/// applied to `returns[j]` (the day `j+1` return), there is no look-ahead.
///
/// The first `lookback` positions are flat (0.0) because no `lookback`-day-ago
/// price exists yet. Output length matches `simple_returns(prices)`.
pub fn momentum_positions(prices: &[f64], lookback: usize) -> Vec<f64> {
    if prices.len() < 2 {
        return Vec::new();
    }
    let n_ret = prices.len() - 1;
    (0..n_ret)
        .map(|j| {
            if j >= lookback && prices[j] > prices[j - lookback] {
                1.0
            } else {
                0.0
            }
        })
        .collect()
}

/// Element-wise strategy returns: `strat[j] = returns[j] * positions[j]`.
///
/// The two slices must be the same length (they are, by construction, when both
/// come from the same price series). Panics on a length mismatch to surface bugs.
pub fn strategy_returns(returns: &[f64], positions: &[f64]) -> Vec<f64> {
    assert_eq!(
        returns.len(),
        positions.len(),
        "returns and positions must align 1:1"
    );
    returns
        .iter()
        .zip(positions)
        .map(|(r, p)| r * p)
        .collect()
}

/// Compounded equity curve starting at 1.0.
///
/// Output length is `returns.len() + 1`; `equity[0] == 1.0` and
/// `equity[k] = equity[k-1] * (1 + returns[k-1])`.
pub fn equity_curve(returns: &[f64]) -> Vec<f64> {
    let mut equity = Vec::with_capacity(returns.len() + 1);
    let mut level = 1.0;
    equity.push(level);
    for r in returns {
        level *= 1.0 + r;
        equity.push(level);
    }
    equity
}

/// Compound annual growth rate from an equity curve.
///
/// `years = (equity.len() - 1) / periods_per_year`. Returns 0.0 if the curve is
/// too short or non-positive (degenerate inputs rather than NaN/inf).
pub fn cagr(equity: &[f64], periods_per_year: f64) -> f64 {
    if equity.len() < 2 {
        return 0.0;
    }
    let first = equity[0];
    let last = *equity.last().unwrap();
    if first <= 0.0 || last <= 0.0 {
        return 0.0;
    }
    let years = (equity.len() - 1) as f64 / periods_per_year;
    if years <= 0.0 {
        return 0.0;
    }
    (last / first).powf(1.0 / years) - 1.0
}

/// Annualized Sharpe ratio of a return series (risk-free rate assumed 0).
///
/// `mean / sample_std * sqrt(periods_per_year)`, using the sample standard
/// deviation (ddof = 1). Returns 0.0 when volatility is zero or undefined.
pub fn annualized_sharpe(returns: &[f64], periods_per_year: f64) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let var = returns
        .iter()
        .map(|r| {
            let d = r - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1.0);
    let std = var.sqrt();
    if std <= 0.0 {
        return 0.0;
    }
    (mean / std) * periods_per_year.sqrt()
}

/// Maximum drawdown of an equity curve, returned as a non-positive fraction.
///
/// e.g. a peak-to-trough fall of 20% returns `-0.20`. A monotonically
/// non-decreasing curve returns `0.0`.
pub fn max_drawdown(equity: &[f64]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd = 0.0_f64;
    for &v in equity {
        if v > peak {
            peak = v;
        }
        if peak > 0.0 {
            let dd = v / peak - 1.0;
            if dd < max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

/// Convenience bundle of the three headline metrics for a return series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub cagr: f64,
    pub sharpe: f64,
    pub max_drawdown: f64,
}

/// Compute CAGR, annualized Sharpe, and max drawdown from a return series.
pub fn metrics(returns: &[f64], periods_per_year: f64) -> Metrics {
    let equity = equity_curve(returns);
    Metrics {
        cagr: cagr(&equity, periods_per_year),
        sharpe: annualized_sharpe(returns, periods_per_year),
        max_drawdown: max_drawdown(&equity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn simple_returns_basic() {
        let r = simple_returns(&[100.0, 110.0, 99.0]);
        assert_eq!(r.len(), 2);
        assert!(approx(r[0], 0.10, 1e-12));
        assert!(approx(r[1], -0.10, 1e-12));
    }

    #[test]
    fn simple_returns_degenerate() {
        assert!(simple_returns(&[]).is_empty());
        assert!(simple_returns(&[42.0]).is_empty());
    }

    #[test]
    fn momentum_positions_alignment_and_lookahead() {
        // prices: index 0..4
        let prices = [1.0, 2.0, 3.0, 4.0, 1.0];
        let pos = momentum_positions(&prices, 2);
        // length must equal simple_returns length (n-1 = 4)
        assert_eq!(pos.len(), simple_returns(&prices).len());
        // j<2 -> flat; j=2: p[2]=3>p[0]=1 -> long; j=3: p[3]=4>p[1]=2 -> long
        assert_eq!(pos, vec![0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn momentum_positions_flat_when_below() {
        // Strictly decreasing -> never long after warmup.
        let prices = [5.0, 4.0, 3.0, 2.0, 1.0];
        let pos = momentum_positions(&prices, 1);
        assert_eq!(pos, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn strategy_returns_masks_flat_days() {
        let returns = vec![0.10, -0.05, 0.20];
        let positions = vec![0.0, 1.0, 1.0];
        let strat = strategy_returns(&returns, &positions);
        assert_eq!(strat, vec![0.0, -0.05, 0.20]);
    }

    #[test]
    #[should_panic(expected = "align")]
    fn strategy_returns_length_mismatch_panics() {
        strategy_returns(&[0.1, 0.2], &[1.0]);
    }

    #[test]
    fn equity_curve_compounds() {
        let eq = equity_curve(&[0.10, -0.10]);
        assert_eq!(eq.len(), 3);
        assert!(approx(eq[0], 1.0, 1e-12));
        assert!(approx(eq[1], 1.10, 1e-12));
        assert!(approx(eq[2], 0.99, 1e-12));
    }

    #[test]
    fn cagr_known_value() {
        // Doubling over exactly one year (ppy = 1) -> 100% CAGR.
        let eq = vec![1.0, 2.0];
        assert!(approx(cagr(&eq, 1.0), 1.0, 1e-12));
        // Quadrupling over two periods at ppy=1 -> 2 years -> CAGR = 100%.
        let eq2 = vec![1.0, 2.0, 4.0];
        assert!(approx(cagr(&eq2, 1.0), 1.0, 1e-12));
    }

    #[test]
    fn cagr_degenerate_is_zero() {
        assert_eq!(cagr(&[1.0], 252.0), 0.0);
        assert_eq!(cagr(&[-1.0, 2.0], 252.0), 0.0);
        assert_eq!(cagr(&[1.0, 2.0], 0.0), 0.0);
    }

    #[test]
    fn sharpe_known_value() {
        // returns [1.0, 3.0]: mean=2, sample std=sqrt(2); ppy=1 -> 2/sqrt(2)=sqrt(2).
        let s = annualized_sharpe(&[1.0, 3.0], 1.0);
        assert!(approx(s, 2.0_f64.sqrt(), 1e-12));
    }

    #[test]
    fn sharpe_zero_vol_is_zero() {
        assert_eq!(annualized_sharpe(&[0.01, 0.01, 0.01], 252.0), 0.0);
        assert_eq!(annualized_sharpe(&[0.01], 252.0), 0.0);
    }

    #[test]
    fn sharpe_annualization_factor() {
        // Same shape, ppy=252 should scale by sqrt(252) vs ppy=1.
        let base = annualized_sharpe(&[1.0, 3.0], 1.0);
        let ann = annualized_sharpe(&[1.0, 3.0], 252.0);
        assert!(approx(ann, base * 252.0_f64.sqrt(), 1e-9));
    }

    #[test]
    fn max_drawdown_known_value() {
        // peak 1.2 then trough 0.6 -> -50%.
        let dd = max_drawdown(&[1.0, 1.2, 0.6, 0.9]);
        assert!(approx(dd, -0.5, 1e-12));
    }

    #[test]
    fn max_drawdown_monotonic_is_zero() {
        assert_eq!(max_drawdown(&[1.0, 1.1, 1.2, 1.3]), 0.0);
    }

    #[test]
    fn metrics_bundle_consistent() {
        let returns = vec![0.10, -0.10, 0.05, 0.05];
        let m = metrics(&returns, 252.0);
        let eq = equity_curve(&returns);
        assert_eq!(m.cagr, cagr(&eq, 252.0));
        assert_eq!(m.sharpe, annualized_sharpe(&returns, 252.0));
        assert_eq!(m.max_drawdown, max_drawdown(&eq));
    }

    #[test]
    fn end_to_end_uptrend_goes_long_and_profits() {
        // Smoothly rising series: after warmup the strategy is fully long and
        // its equity should match buy-and-hold over the long portion.
        let prices: Vec<f64> = (0..30).map(|i| 100.0 * 1.01_f64.powi(i)).collect();
        let returns = simple_returns(&prices);
        let pos = momentum_positions(&prices, 5);
        let strat = strategy_returns(&returns, &pos);
        // Every post-warmup day is an uptrend day -> long.
        assert!(pos[5..].iter().all(|&p| p == 1.0));
        let m = metrics(&strat, 252.0);
        assert!(m.cagr > 0.0);
        assert!(m.max_drawdown <= 0.0);
    }
}

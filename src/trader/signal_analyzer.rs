/// Trading signal types
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Buy,
    Sell,
    Hold,
}

/// Analyzes indicator data to generate trading signals
pub struct SignalAnalyzer;

impl SignalAnalyzer {
    /// Detect golden cross (buy) or death cross (sell) from two moving averages
    ///
    /// Golden Cross: Short-term MA crosses above long-term MA → BUY
    /// Death Cross: Short-term MA crosses below long-term MA → SELL
    pub fn detect_ma_cross(short_ma: &[f64], long_ma: &[f64]) -> Signal {
        if short_ma.len() < 2 || long_ma.len() < 2 {
            return Signal::Hold;
        }

        let prev_short = short_ma[short_ma.len() - 2];
        let curr_short = short_ma[short_ma.len() - 1];
        let prev_long = long_ma[long_ma.len() - 2];
        let curr_long = long_ma[long_ma.len() - 1];

        // Golden cross: short was below or equal, now above
        if prev_short <= prev_long && curr_short > curr_long {
            return Signal::Buy;
        }

        // Death cross: short was above or equal, now below
        if prev_short >= prev_long && curr_short < curr_long {
            return Signal::Sell;
        }

        Signal::Hold
    }

    /// Detect RSI overbought/oversold signals
    ///
    /// RSI > overbought_threshold (default 70) → SELL
    /// RSI < oversold_threshold (default 30) → BUY
    pub fn detect_rsi_signal(
        rsi: &[f64],
        oversold_threshold: f64,
        overbought_threshold: f64,
    ) -> Signal {
        if rsi.is_empty() {
            return Signal::Hold;
        }

        let current_rsi = rsi[rsi.len() - 1];

        if current_rsi > overbought_threshold {
            Signal::Sell
        } else if current_rsi < oversold_threshold {
            Signal::Buy
        } else {
            Signal::Hold
        }
    }

    /// Detect Bollinger Band breakout signals
    ///
    /// Price breaks above upper band → SELL (overbought)
    /// Price breaks below lower band → BUY (oversold)
    ///
    /// Note: bb should be in format [middle, upper, lower, middle, upper, lower, ...]
    pub fn detect_bb_breakout(current_price: f64, bb: &[f64]) -> Signal {
        if bb.len() < 3 {
            return Signal::Hold;
        }

        // Get the last set of bands (last 3 values)
        let upper_band = bb[bb.len() - 2];
        let lower_band = bb[bb.len() - 1];

        if current_price > upper_band {
            Signal::Sell // Overbought
        } else if current_price < lower_band {
            Signal::Buy // Oversold
        } else {
            Signal::Hold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_golden_cross() {
        // Short MA crosses above long MA
        let short_ma = vec![95.0, 105.0];
        let long_ma = vec![100.0, 100.0];

        let signal = SignalAnalyzer::detect_ma_cross(&short_ma, &long_ma);
        assert_eq!(signal, Signal::Buy);
    }

    #[test]
    fn test_death_cross() {
        // Short MA crosses below long MA
        let short_ma = vec![105.0, 95.0];
        let long_ma = vec![100.0, 100.0];

        let signal = SignalAnalyzer::detect_ma_cross(&short_ma, &long_ma);
        assert_eq!(signal, Signal::Sell);
    }

    #[test]
    fn test_no_cross() {
        // No crossover
        let short_ma = vec![105.0, 106.0];
        let long_ma = vec![100.0, 100.0];

        let signal = SignalAnalyzer::detect_ma_cross(&short_ma, &long_ma);
        assert_eq!(signal, Signal::Hold);
    }

    #[test]
    fn test_rsi_oversold() {
        let rsi = vec![50.0, 40.0, 25.0];
        let signal = SignalAnalyzer::detect_rsi_signal(&rsi, 30.0, 70.0);
        assert_eq!(signal, Signal::Buy);
    }

    #[test]
    fn test_rsi_overbought() {
        let rsi = vec![50.0, 60.0, 75.0];
        let signal = SignalAnalyzer::detect_rsi_signal(&rsi, 30.0, 70.0);
        assert_eq!(signal, Signal::Sell);
    }

    #[test]
    fn test_rsi_neutral() {
        let rsi = vec![50.0, 55.0, 60.0];
        let signal = SignalAnalyzer::detect_rsi_signal(&rsi, 30.0, 70.0);
        assert_eq!(signal, Signal::Hold);
    }

    #[test]
    fn test_bb_breakout_above() {
        // Price above upper band
        let bb = vec![100.0, 110.0, 90.0]; // [middle, upper, lower]
        let signal = SignalAnalyzer::detect_bb_breakout(115.0, &bb);
        assert_eq!(signal, Signal::Sell);
    }

    #[test]
    fn test_bb_breakout_below() {
        // Price below lower band
        let bb = vec![100.0, 110.0, 90.0];
        let signal = SignalAnalyzer::detect_bb_breakout(85.0, &bb);
        assert_eq!(signal, Signal::Buy);
    }

    #[test]
    fn test_bb_within_bands() {
        // Price within bands
        let bb = vec![100.0, 110.0, 90.0];
        let signal = SignalAnalyzer::detect_bb_breakout(100.0, &bb);
        assert_eq!(signal, Signal::Hold);
    }
}

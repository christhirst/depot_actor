use crate::signal_analyzer::Signal;

/// Weighted signal from a strategy
#[derive(Debug, Clone)]
pub struct WeightedSignal {
    pub strategy_name: String,
    pub signal: Signal,
    pub weight: f64,
    pub confidence: f64, // 0.0 to 1.0
}

/// Aggregated signal with strength
#[derive(Debug, Clone, PartialEq)]
pub enum AggregatedSignal {
    Buy { strength: f64 },
    Sell { strength: f64 },
    Hold,
}

/// Aggregates multiple weighted signals into a final decision
pub struct SignalAggregator {
    buy_threshold: f64,  // e.g., 0.6 = 60% weighted buy signals needed
    sell_threshold: f64, // e.g., 0.6 = 60% weighted sell signals needed
}

impl SignalAggregator {
    pub fn new(buy_threshold: f64, sell_threshold: f64) -> Self {
        Self {
            buy_threshold,
            sell_threshold,
        }
    }

    /// Aggregate multiple weighted signals into a final decision
    pub fn aggregate(&self, signals: Vec<WeightedSignal>) -> AggregatedSignal {
        if signals.is_empty() {
            return AggregatedSignal::Hold;
        }

        let total_weight: f64 = signals.iter().map(|s| s.weight).sum();

        if total_weight == 0.0 {
            return AggregatedSignal::Hold;
        }

        // Calculate weighted scores
        let buy_score: f64 = signals
            .iter()
            .filter(|s| s.signal == Signal::Buy)
            .map(|s| s.weight * s.confidence)
            .sum::<f64>()
            / total_weight;

        let sell_score: f64 = signals
            .iter()
            .filter(|s| s.signal == Signal::Sell)
            .map(|s| s.weight * s.confidence)
            .sum::<f64>()
            / total_weight;

        tracing::debug!(
            "[AGGREGATOR] Buy score: {:.2}, Sell score: {:.2}",
            buy_score,
            sell_score
        );

        // Determine final signal based on thresholds
        if buy_score >= self.buy_threshold && buy_score > sell_score {
            AggregatedSignal::Buy {
                strength: buy_score,
            }
        } else if sell_score >= self.sell_threshold && sell_score > buy_score {
            AggregatedSignal::Sell {
                strength: sell_score,
            }
        } else {
            AggregatedSignal::Hold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_strong_buy() {
        let aggregator = SignalAggregator::new(0.6, 0.6);

        let signals = vec![
            WeightedSignal {
                strategy_name: "MA Cross".to_string(),
                signal: Signal::Buy,
                weight: 0.4,
                confidence: 1.0,
            },
            WeightedSignal {
                strategy_name: "RSI".to_string(),
                signal: Signal::Buy,
                weight: 0.3,
                confidence: 1.0,
            },
            WeightedSignal {
                strategy_name: "BB".to_string(),
                signal: Signal::Hold,
                weight: 0.3,
                confidence: 1.0,
            },
        ];

        let result = aggregator.aggregate(signals);
        match result {
            AggregatedSignal::Buy { strength } => {
                // (0.4 + 0.3) / 1.0 = 0.7
                assert!((strength - 0.7).abs() < 0.01);
            }
            _ => panic!("Expected Buy signal"),
        }
    }

    #[test]
    fn test_aggregate_weak_buy() {
        let aggregator = SignalAggregator::new(0.6, 0.6);

        let signals = vec![
            WeightedSignal {
                strategy_name: "MA Cross".to_string(),
                signal: Signal::Buy,
                weight: 0.3,
                confidence: 1.0,
            },
            WeightedSignal {
                strategy_name: "RSI".to_string(),
                signal: Signal::Hold,
                weight: 0.4,
                confidence: 1.0,
            },
            WeightedSignal {
                strategy_name: "BB".to_string(),
                signal: Signal::Sell,
                weight: 0.3,
                confidence: 1.0,
            },
        ];

        let result = aggregator.aggregate(signals);
        // Buy score: 0.3 / 1.0 = 0.3 (below threshold)
        assert_eq!(result, AggregatedSignal::Hold);
    }

    #[test]
    fn test_aggregate_strong_sell() {
        let aggregator = SignalAggregator::new(0.6, 0.6);

        let signals = vec![
            WeightedSignal {
                strategy_name: "MA Cross".to_string(),
                signal: Signal::Sell,
                weight: 0.5,
                confidence: 1.0,
            },
            WeightedSignal {
                strategy_name: "RSI".to_string(),
                signal: Signal::Sell,
                weight: 0.3,
                confidence: 1.0,
            },
            WeightedSignal {
                strategy_name: "BB".to_string(),
                signal: Signal::Hold,
                weight: 0.2,
                confidence: 1.0,
            },
        ];

        let result = aggregator.aggregate(signals);
        match result {
            AggregatedSignal::Sell { strength } => {
                // (0.5 + 0.3) / 1.0 = 0.8
                assert!((strength - 0.8).abs() < 0.01);
            }
            _ => panic!("Expected Sell signal"),
        }
    }

    #[test]
    fn test_aggregate_with_confidence() {
        let aggregator = SignalAggregator::new(0.6, 0.6);

        let signals = vec![
            WeightedSignal {
                strategy_name: "MA Cross".to_string(),
                signal: Signal::Buy,
                weight: 0.5,
                confidence: 0.8, // 80% confident
            },
            WeightedSignal {
                strategy_name: "RSI".to_string(),
                signal: Signal::Buy,
                weight: 0.5,
                confidence: 0.6, // 60% confident
            },
        ];

        let result = aggregator.aggregate(signals);
        match result {
            AggregatedSignal::Buy { strength } => {
                // (0.5 * 0.8 + 0.5 * 0.6) / 1.0 = 0.7
                assert!((strength - 0.7).abs() < 0.01);
            }
            _ => panic!("Expected Buy signal"),
        }
    }

    #[test]
    fn test_aggregate_empty_signals() {
        let aggregator = SignalAggregator::new(0.6, 0.6);
        let result = aggregator.aggregate(vec![]);
        assert_eq!(result, AggregatedSignal::Hold);
    }
}

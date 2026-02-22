use crate::settings::SymbolConfig;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

/// Thread-safe buffer for storing recent market data per symbol
pub struct DataBuffer {
    buffers: Arc<RwLock<HashMap<String, SymbolBuffer>>>,
}

struct SymbolBuffer {
    prices: VecDeque<f64>,
    max_size: usize,
}

impl DataBuffer {
    /// Create a new DataBuffer from symbol configurations
    pub fn new(symbols: &[SymbolConfig]) -> Self {
        let mut buffers = HashMap::new();

        for symbol in symbols {
            buffers.insert(
                symbol.name.clone(),
                SymbolBuffer {
                    prices: VecDeque::with_capacity(symbol.buffer_size),
                    max_size: symbol.buffer_size,
                },
            );
        }

        Self {
            buffers: Arc::new(RwLock::new(buffers)),
        }
    }

    /// Add a price data point for a symbol
    pub fn add_price(&self, symbol: &str, price: f64) {
        let mut buffers = self.buffers.write().unwrap();

        if let Some(buffer) = buffers.get_mut(symbol) {
            buffer.prices.push_back(price);

            // Remove oldest if exceeds max size
            if buffer.prices.len() > buffer.max_size {
                buffer.prices.pop_front();
            }
        }
    }

    /// Get all buffered prices for a symbol
    pub fn get_prices(&self, symbol: &str) -> Vec<f64> {
        let buffers = self.buffers.read().unwrap();

        buffers
            .get(symbol)
            .map(|buffer| buffer.prices.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get the latest N prices for a symbol
    pub fn get_latest_n(&self, symbol: &str, n: usize) -> Vec<f64> {
        let buffers = self.buffers.read().unwrap();

        buffers
            .get(symbol)
            .map(|buffer| {
                buffer
                    .prices
                    .iter()
                    .rev()
                    .take(n)
                    .copied()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn len(&self, symbol: &str) -> usize {
        let buffers = self.buffers.read().unwrap();

        buffers
            .get(symbol)
            .map(|buffer| buffer.prices.len())
            .unwrap_or(0)
    }

    /// Initialize the data buffer with historical data from the database
    pub async fn init_from_db(&self, db: &crate::data::db::Database) -> anyhow::Result<()> {
        let symbol_names: Vec<String> = {
            let buffers = self.buffers.read().unwrap();
            buffers.keys().cloned().collect()
        };

        for symbol in symbol_names {
            // First try to get aggregated bars as they are more relevant for indicators
            match db.get_bars(&symbol).await {
                Ok(bars) if !bars.is_empty() => {
                    let mut buffers = self.buffers.write().unwrap();
                    if let Some(buffer) = buffers.get_mut(&symbol) {
                        let max_size = buffer.max_size;
                        // Sort bars by time just in case, but get_bars usually returns them in order
                        // The tuple format is (symbol, open, high, low, close, volume)
                        // We take the last max_size elements
                        let iter = bars.iter().rev().take(max_size).collect::<Vec<_>>();
                        for bar in iter.into_iter().rev() {
                            buffer.prices.push_back(bar.4); // close price is 4th index
                        }
                        tracing::info!(
                            "Pre-filled DataBuffer for {} with {} historical bars",
                            symbol,
                            buffer.prices.len() // Use the actual number pushed
                        );
                    }
                }
                _ => {
                    // Try getting raw trades if no bars exist
                    match db.get_trades(&symbol).await {
                        Ok(trades) if !trades.is_empty() => {
                            let mut buffers = self.buffers.write().unwrap();
                            if let Some(buffer) = buffers.get_mut(&symbol) {
                                let max_size = buffer.max_size;
                                let iter = trades.iter().rev().take(max_size).collect::<Vec<_>>();
                                for trade in iter.into_iter().rev() {
                                    buffer.prices.push_back(trade.1); // price is 1st index
                                }
                                tracing::info!(
                                    "Pre-filled DataBuffer for {} with {} historical trades",
                                    symbol,
                                    buffer.prices.len() // Use the actual number pushed
                                );
                            }
                        }
                        _ => {
                            tracing::warn!("No historical data found for symbol {}", symbol);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SymbolConfig;

    fn create_test_symbol(name: &str, buffer_size: usize) -> SymbolConfig {
        SymbolConfig {
            name: name.to_string(),
            subscribe_trades: true,
            subscribe_bars: true,
            aggregation_interval_hours: 24,
            buffer_size,
        }
    }

    #[test]
    fn test_buffer_creation() {
        let symbols = vec![
            create_test_symbol("AAPL", 100),
            create_test_symbol("GOOGL", 200),
        ];

        let buffer = DataBuffer::new(&symbols);
        assert_eq!(buffer.len("AAPL"), 0);
        assert_eq!(buffer.len("GOOGL"), 0);
    }

    #[test]
    fn test_add_and_get_prices() {
        let symbols = vec![create_test_symbol("AAPL", 5)];
        let buffer = DataBuffer::new(&symbols);

        buffer.add_price("AAPL", 100.0);
        buffer.add_price("AAPL", 101.0);
        buffer.add_price("AAPL", 102.0);

        let prices = buffer.get_prices("AAPL");
        assert_eq!(prices, vec![100.0, 101.0, 102.0]);
        assert_eq!(buffer.len("AAPL"), 3);
    }

    #[test]
    fn test_buffer_size_limit() {
        let symbols = vec![create_test_symbol("AAPL", 3)];
        let buffer = DataBuffer::new(&symbols);

        buffer.add_price("AAPL", 100.0);
        buffer.add_price("AAPL", 101.0);
        buffer.add_price("AAPL", 102.0);
        buffer.add_price("AAPL", 103.0);
        buffer.add_price("AAPL", 104.0);

        let prices = buffer.get_prices("AAPL");
        assert_eq!(prices, vec![102.0, 103.0, 104.0]);
        assert_eq!(buffer.len("AAPL"), 3);
    }

    #[test]
    fn test_get_latest_n() {
        let symbols = vec![create_test_symbol("AAPL", 10)];
        let buffer = DataBuffer::new(&symbols);

        for i in 0..7 {
            buffer.add_price("AAPL", 100.0 + i as f64);
        }

        let latest_3 = buffer.get_latest_n("AAPL", 3);
        assert_eq!(latest_3, vec![104.0, 105.0, 106.0]);

        let latest_10 = buffer.get_latest_n("AAPL", 10);
        assert_eq!(latest_10.len(), 7); // Only 7 available
    }
}

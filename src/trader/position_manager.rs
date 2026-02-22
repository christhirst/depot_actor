use crate::trader::broker_client::{BrokerClient, Order};
use anyhow::Result;
use std::sync::Arc;

/// Manages trading positions and risk
pub struct PositionManager {
    broker: Arc<dyn BrokerClient>,
    max_position_size_pct: f64,  // Max % of portfolio per long position
    max_total_exposure_pct: f64, // Max % of cash that can be invested in longs
    max_short_position_pct: f64, // Max % of portfolio per short position
    max_short_exposure_pct: f64, // Max % of portfolio in short positions
}

impl PositionManager {
    pub fn new(
        broker: Arc<dyn BrokerClient>,
        max_position_size_pct: f64,
        max_total_exposure_pct: f64,
        max_short_position_pct: f64,
        max_short_exposure_pct: f64,
    ) -> Self {
        Self {
            broker,
            max_position_size_pct,
            max_total_exposure_pct,
            max_short_position_pct,
            max_short_exposure_pct,
        }
    }

    /// Calculate position size for long positions based on available cash and risk limits
    pub async fn calculate_position_size(&self, symbol: &str, current_price: f64) -> Result<f64> {
        let balance = self.broker.get_balance().await?;
        let positions = self.broker.get_positions().await?;

        // Calculate current long exposure (total value of all long positions)
        let total_exposure: f64 = positions
            .iter()
            .filter(|p| p.position_type == crate::trader::broker_client::PositionType::Long)
            .map(|p| p.quantity * p.current_price)
            .sum();

        // Available cash for new positions
        let max_investable = balance * self.max_total_exposure_pct;
        let available = max_investable - total_exposure;

        if available <= 0.0 {
            tracing::warn!(
                "[POSITION] No available cash for new positions (exposure: ${:.2}/{:.2})",
                total_exposure,
                max_investable
            );
            return Ok(0.0);
        }

        // Max for this specific position
        let max_position_value = balance * self.max_position_size_pct;

        // Take the minimum of available cash and max position size
        let position_value = available.min(max_position_value);
        let quantity = (position_value / current_price).floor();

        tracing::debug!(
            "[POSITION] Calculated size for {}: {} shares (${:.2} at ${:.2}/share)",
            symbol,
            quantity,
            position_value,
            current_price
        );

        Ok(quantity)
    }

    /// Execute a buy order
    pub async fn execute_buy(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        if quantity <= 0.0 {
            anyhow::bail!("Invalid quantity: {}", quantity);
        }

        let order = self.broker.buy(symbol, quantity, price).await?;
        tracing::info!(
            "[POSITION] Buy executed: {} shares of {} at ${:.2}",
            quantity,
            symbol,
            price
        );

        Ok(order)
    }

    /// Execute a sell order
    pub async fn execute_sell(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        if quantity <= 0.0 {
            anyhow::bail!("Invalid quantity: {}", quantity);
        }

        let order = self.broker.sell(symbol, quantity, price).await?;
        tracing::info!(
            "[POSITION] Sell executed: {} shares of {} at ${:.2}",
            quantity,
            symbol,
            price
        );

        Ok(order)
    }

    /// Calculate position size for short positions based on risk limits
    pub async fn calculate_short_position_size(
        &self,
        symbol: &str,
        current_price: f64,
    ) -> Result<f64> {
        let balance = self.broker.get_balance().await?;
        let positions = self.broker.get_positions().await?;

        // Calculate current short exposure
        let short_exposure: f64 = positions
            .iter()
            .filter(|p| p.position_type == crate::trader::broker_client::PositionType::Short)
            .map(|p| p.quantity * p.current_price)
            .sum();

        // Max short exposure allowed
        let max_short_investable = balance * self.max_short_exposure_pct;
        let available = max_short_investable - short_exposure;

        if available <= 0.0 {
            tracing::warn!(
                "[POSITION] No available capacity for new shorts (exposure: ${:.2}/{:.2})",
                short_exposure,
                max_short_investable
            );
            return Ok(0.0);
        }

        // Max for this specific short position (more conservative than longs)
        let max_position_value = balance * self.max_short_position_pct;

        // Take the minimum of available capacity and max position size
        let position_value = available.min(max_position_value);
        let quantity = (position_value / current_price).floor();

        tracing::debug!(
            "[POSITION] Calculated short size for {}: {} shares (${:.2} at ${:.2}/share)",
            symbol,
            quantity,
            position_value,
            current_price
        );

        Ok(quantity)
    }

    /// Execute a short order
    pub async fn execute_short(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        if quantity <= 0.0 {
            anyhow::bail!("Invalid quantity: {}", quantity);
        }

        let order = self.broker.short(symbol, quantity, price).await?;
        tracing::info!(
            "[POSITION] Short executed: {} shares of {} at ${:.2}",
            quantity,
            symbol,
            price
        );

        Ok(order)
    }

    /// Execute a cover order (close short position)
    pub async fn execute_cover(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        if quantity <= 0.0 {
            anyhow::bail!("Invalid quantity: {}", quantity);
        }

        let order = self.broker.cover(symbol, quantity, price).await?;
        tracing::info!(
            "[POSITION] Cover executed: {} shares of {} at ${:.2}",
            quantity,
            symbol,
            price
        );

        Ok(order)
    }

    /// Get current position for a symbol (returns quantity and type)
    pub async fn get_position(
        &self,
        symbol: &str,
    ) -> Result<Option<(f64, crate::trader::broker_client::PositionType)>> {
        let positions = self.broker.get_positions().await?;
        let position = positions.iter().find(|p| p.symbol == symbol);
        Ok(position.map(|p| (p.quantity, p.position_type.clone())))
    }

    /// Get reference to broker client
    pub fn broker(&self) -> &Arc<dyn BrokerClient> {
        &self.broker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::broker_client::{OrderSide, Position};
    use async_trait::async_trait;

    struct MockBroker {
        balance: f64,
        positions: Vec<Position>,
    }

    #[async_trait]
    impl BrokerClient for MockBroker {
        async fn get_balance(&self) -> Result<f64> {
            Ok(self.balance)
        }

        async fn get_positions(&self) -> Result<Vec<Position>> {
            Ok(self.positions.clone())
        }

        async fn buy(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
            Ok(Order {
                symbol: symbol.to_string(),
                quantity,
                price,
                side: OrderSide::Buy,
            })
        }

        async fn sell(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
            Ok(Order {
                symbol: symbol.to_string(),
                quantity,
                price,
                side: OrderSide::Sell,
            })
        }

        async fn short(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
            Ok(Order {
                symbol: symbol.to_string(),
                quantity,
                price,
                side: OrderSide::Short,
            })
        }

        async fn cover(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
            Ok(Order {
                symbol: symbol.to_string(),
                quantity,
                price,
                side: OrderSide::Cover,
            })
        }
    }

    #[tokio::test]
    async fn test_calculate_position_size_no_positions() {
        let broker = Arc::new(MockBroker {
            balance: 10000.0,
            positions: vec![],
        });

        let manager = PositionManager::new(broker, 0.10, 0.80, 0.05, 0.50);
        let size = manager
            .calculate_position_size("AAPL", 100.0)
            .await
            .unwrap();

        // Max position is 10% of 10000 = 1000
        // At $100/share, that's 10 shares
        assert_eq!(size, 10.0);
    }

    #[tokio::test]
    async fn test_calculate_position_size_with_existing_positions() {
        let broker = Arc::new(MockBroker {
            balance: 10000.0,
            positions: vec![Position {
                symbol: "TSLA".to_string(),
                quantity: 20.0,
                avg_price: 200.0,
                current_price: 200.0,
                position_type: crate::trader::broker_client::PositionType::Long,
            }],
        });

        let manager = PositionManager::new(broker, 0.10, 0.80, 0.05, 0.50);
        let size = manager
            .calculate_position_size("AAPL", 100.0)
            .await
            .unwrap();

        // Total exposure allowed: 80% of 10000 = 8000
        // Current exposure: 20 * 200 = 4000
        // Available: 8000 - 4000 = 4000
        // Max position: 10% of 10000 = 1000
        // Take minimum: 1000
        // At $100/share: 10 shares
        assert_eq!(size, 10.0);
    }

    #[tokio::test]
    async fn test_calculate_position_size_max_exposure_reached() {
        let broker = Arc::new(MockBroker {
            balance: 10000.0,
            positions: vec![Position {
                symbol: "TSLA".to_string(),
                quantity: 40.0,
                avg_price: 200.0,
                current_price: 200.0,
                position_type: crate::trader::broker_client::PositionType::Long,
            }],
        });

        let manager = PositionManager::new(broker, 0.10, 0.80, 0.05, 0.50);
        let size = manager
            .calculate_position_size("AAPL", 100.0)
            .await
            .unwrap();

        // Total exposure allowed: 80% of 10000 = 8000
        // Current exposure: 40 * 200 = 8000
        // Available: 0
        assert_eq!(size, 0.0);
    }
}

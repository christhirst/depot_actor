use crate::data_buffer::DataBuffer;
use crate::indicator_client::IndicatorClient;
use crate::trader::signal_analyzer::{Signal, SignalAnalyzer};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Trait for trading strategies
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    async fn evaluate(
        &self,
        symbol: &str,
        data_buffer: &DataBuffer,
        indicator_client: &mut IndicatorClient,
    ) -> Result<Signal>;

    fn name(&self) -> &str;
}

/// Moving Average Crossover Strategy (Golden Cross / Death Cross)
pub struct MovingAverageCrossStrategy {
    short_period: usize,
    long_period: usize,
}

impl MovingAverageCrossStrategy {
    pub fn new(short_period: usize, long_period: usize) -> Self {
        Self {
            short_period,
            long_period,
        }
    }
}

#[async_trait]
impl TradingStrategy for MovingAverageCrossStrategy {
    async fn evaluate(
        &self,
        symbol: &str,
        data_buffer: &DataBuffer,
        indicator_client: &mut IndicatorClient,
    ) -> Result<Signal> {
        // Get buffered prices
        let prices = data_buffer.get_prices(symbol);

        if prices.len() < self.long_period {
            tracing::debug!(
                "[MA Cross] Not enough data for {}: {} < {}",
                symbol,
                prices.len(),
                self.long_period
            );
            return Ok(Signal::Hold);
        }

        // Calculate short and long moving averages
        let short_ma = indicator_client
            .calculate_sma(prices.clone(), self.short_period)
            .await?;
        let long_ma = indicator_client
            .calculate_sma(prices.clone(), self.long_period)
            .await?;

        // Detect crossover
        let signal = SignalAnalyzer::detect_ma_cross(&short_ma, &long_ma);

        Ok(signal)
    }

    fn name(&self) -> &str {
        "MA Cross"
    }
}

/// RSI Strategy (Overbought/Oversold)
pub struct RsiStrategy {
    period: usize,
    oversold_threshold: f64,
    overbought_threshold: f64,
}

impl RsiStrategy {
    pub fn new(period: usize, oversold_threshold: f64, overbought_threshold: f64) -> Self {
        Self {
            period,
            oversold_threshold,
            overbought_threshold,
        }
    }
}

#[async_trait]
impl TradingStrategy for RsiStrategy {
    async fn evaluate(
        &self,
        symbol: &str,
        data_buffer: &DataBuffer,
        indicator_client: &mut IndicatorClient,
    ) -> Result<Signal> {
        let prices = data_buffer.get_prices(symbol);

        if prices.len() < self.period + 1 {
            return Ok(Signal::Hold);
        }

        let rsi = indicator_client.calculate_rsi(prices, self.period).await?;

        let signal = SignalAnalyzer::detect_rsi_signal(
            &rsi,
            self.oversold_threshold,
            self.overbought_threshold,
        );

        Ok(signal)
    }

    fn name(&self) -> &str {
        "RSI"
    }
}

/// Bollinger Bands Strategy
pub struct BollingerBandsStrategy {
    period: usize,
    multiplier: f64,
}

impl BollingerBandsStrategy {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self { period, multiplier }
    }
}

#[async_trait]
impl TradingStrategy for BollingerBandsStrategy {
    async fn evaluate(
        &self,
        symbol: &str,
        data_buffer: &DataBuffer,
        indicator_client: &mut IndicatorClient,
    ) -> Result<Signal> {
        let prices = data_buffer.get_prices(symbol);

        if prices.len() < self.period {
            return Ok(Signal::Hold);
        }

        let current_price = *prices.last().unwrap();
        let bb = indicator_client
            .calculate_bollinger_bands(prices, self.period, self.multiplier)
            .await?;

        let signal = SignalAnalyzer::detect_bb_breakout(current_price, &bb);

        Ok(signal)
    }

    fn name(&self) -> &str {
        "Bollinger Bands"
    }
}

/// Trading service that orchestrates strategy evaluation
pub struct TradingService {
    data_buffer: Arc<DataBuffer>,
    indicator_client: IndicatorClient,
    strategies: Vec<(Box<dyn TradingStrategy>, f64)>, // (strategy, weight)
    signal_aggregator: crate::trader::signal_aggregator::SignalAggregator,
    position_manager: crate::trader::position_manager::PositionManager,
    notifier: Arc<dyn crate::trader::notification::Notifier>,
}

impl TradingService {
    pub fn new(
        data_buffer: Arc<DataBuffer>,
        indicator_client: IndicatorClient,
        signal_aggregator: crate::trader::signal_aggregator::SignalAggregator,
        position_manager: crate::trader::position_manager::PositionManager,
        notifier: Arc<dyn crate::trader::notification::Notifier>,
    ) -> Self {
        Self {
            data_buffer,
            indicator_client,
            strategies: Vec::new(),
            signal_aggregator,
            position_manager,
            notifier,
        }
    }

    pub fn add_strategy(&mut self, strategy: Box<dyn TradingStrategy>, weight: f64) {
        self.strategies.push((strategy, weight));
    }

    /// Evaluate all strategies for a symbol and execute if signal is strong enough
    pub async fn evaluate_and_execute(&mut self, symbol: &str) -> Result<()> {
        // 1. Evaluate all strategies
        let mut weighted_signals = Vec::new();

        for (strategy, weight) in &self.strategies {
            match strategy
                .evaluate(symbol, &self.data_buffer, &mut self.indicator_client)
                .await
            {
                Ok(signal) => {
                    if signal != Signal::Hold {
                        tracing::debug!(
                            "[SIGNAL] {} - {}: {:?} (weight: {})",
                            symbol,
                            strategy.name(),
                            signal,
                            weight
                        );
                    }
                    weighted_signals.push(crate::trader::signal_aggregator::WeightedSignal {
                        strategy_name: strategy.name().to_string(),
                        signal,
                        weight: *weight,
                        confidence: 1.0,
                    });
                }
                Err(e) => {
                    tracing::error!(
                        "[SIGNAL] Error evaluating {} for {}: {:?}",
                        strategy.name(),
                        symbol,
                        e
                    );
                }
            }
        }

        // 2. Aggregate signals
        let aggregated = self.signal_aggregator.aggregate(weighted_signals);

        // Get current price
        let prices = self.data_buffer.get_prices(symbol);
        if prices.is_empty() {
            tracing::warn!("[EXECUTE] No price data for {}", symbol);
            return Ok(());
        }
        let price = *prices.last().unwrap();

        // Get current position
        let position = self.position_manager.get_position(symbol).await?;

        // 3. Execute based on aggregated signal and current position
        match aggregated {
            crate::trader::signal_aggregator::AggregatedSignal::Buy { strength } => {
                tracing::info!(
                    "[AGGREGATED] {} - BUY signal (strength: {:.2})",
                    symbol,
                    strength
                );

                match position {
                    Some((quantity, crate::trader::broker_client::PositionType::Short)) => {
                        // Cover short position
                        match self
                            .position_manager
                            .execute_cover(symbol, quantity, price)
                            .await
                        {
                            Ok(_) => {
                                let message = format!(
                                    "COVER {} shares of {} at ${:.2} (strength: {:.2})",
                                    quantity, symbol, price, strength
                                );
                                self.notifier.notify(&message);
                            }
                            Err(e) => {
                                tracing::error!("[EXECUTE] Cover failed for {}: {:?}", symbol, e);
                            }
                        }
                    }
                    Some((_, crate::trader::broker_client::PositionType::Long)) => {
                        tracing::debug!("[EXECUTE] Already long {}, holding", symbol);
                    }
                    None => {
                        // Open long position
                        let quantity = self
                            .position_manager
                            .calculate_position_size(symbol, price)
                            .await?;

                        if quantity > 0.0 {
                            match self
                                .position_manager
                                .execute_buy(symbol, quantity, price)
                                .await
                            {
                                Ok(_) => {
                                    let message = format!(
                                        "BUY {} shares of {} at ${:.2} (strength: {:.2})",
                                        quantity, symbol, price, strength
                                    );
                                    self.notifier.notify(&message);
                                }
                                Err(e) => {
                                    tracing::error!("[EXECUTE] Buy failed for {}: {:?}", symbol, e);
                                }
                            }
                        } else {
                            tracing::debug!("[EXECUTE] Position size is 0 for {}", symbol);
                        }
                    }
                }
            }
            crate::trader::signal_aggregator::AggregatedSignal::Sell { strength } => {
                tracing::info!(
                    "[AGGREGATED] {} - SELL signal (strength: {:.2})",
                    symbol,
                    strength
                );

                match position {
                    Some((quantity, crate::trader::broker_client::PositionType::Long)) => {
                        // Close long position
                        match self
                            .position_manager
                            .execute_sell(symbol, quantity, price)
                            .await
                        {
                            Ok(_) => {
                                let message = format!(
                                    "SELL {} shares of {} at ${:.2} (strength: {:.2})",
                                    quantity, symbol, price, strength
                                );
                                self.notifier.notify(&message);
                            }
                            Err(e) => {
                                tracing::error!("[EXECUTE] Sell failed for {}: {:?}", symbol, e);
                            }
                        }
                    }
                    Some((_, crate::trader::broker_client::PositionType::Short)) => {
                        tracing::debug!("[EXECUTE] Already short {}, holding", symbol);
                    }
                    None => {
                        // Open short position
                        let quantity = self
                            .position_manager
                            .calculate_short_position_size(symbol, price)
                            .await?;

                        if quantity > 0.0 {
                            match self
                                .position_manager
                                .execute_short(symbol, quantity, price)
                                .await
                            {
                                Ok(_) => {
                                    let message = format!(
                                        "SHORT {} shares of {} at ${:.2} (strength: {:.2})",
                                        quantity, symbol, price, strength
                                    );
                                    self.notifier.notify(&message);
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "[EXECUTE] Short failed for {}: {:?}",
                                        symbol,
                                        e
                                    );
                                }
                            }
                        } else {
                            tracing::debug!("[EXECUTE] Short position size is 0 for {}", symbol);
                        }
                    }
                }
            }
            crate::trader::signal_aggregator::AggregatedSignal::Hold => {
                tracing::debug!("[AGGREGATED] {} - HOLD signal", symbol);
            }
        }

        Ok(())
    }
}

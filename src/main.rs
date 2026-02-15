mod aggregator;
mod broker_client;
mod config_reloader;
mod data_buffer;
mod db;
#[cfg(test)]
// mod db_test; // Disabled - requires alpaca_api_client
mod grpc_server;
mod indicator_client;
mod indicator_server;
mod notification;
mod position_manager;
mod settings;
mod signal_aggregator;
mod signal_analyzer;
mod streamer;
mod trading_strategy;

use crate::aggregator::Aggregator;
use crate::config_reloader::ConfigReloader;
use crate::data_buffer::DataBuffer;
use crate::db::Database;
use crate::grpc_server::start_grpc_server;
use crate::indicator_client::IndicatorClient;
use crate::indicator_server::start_indicator_server;
use crate::settings::Settings;
use crate::streamer::Streamer;
use crate::trading_strategy::{
    BollingerBandsStrategy, MovingAverageCrossStrategy, RsiStrategy, TradingService,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting depot_actor...");

    // Create config reloader
    let config_reloader =
        ConfigReloader::new(Settings::load).expect("Failed to load initial configuration");

    let settings = config_reloader.get();

    // Initialize tracing with configured log level
    let log_level = settings
        .log_level
        .parse::<tracing::Level>()
        .unwrap_or(tracing::Level::WARN);

    tracing_subscriber::fmt().with_max_level(log_level).init();

    tracing::info!("Settings loaded: {:?}", settings);

    let db_conn_str = settings.database.connection_string();
    tracing::info!("Connecting to database...");

    let database = Database::new(&db_conn_str).await?;
    let database = Arc::new(database);
    tracing::info!("Database initialized.");

    // Create data buffer from settings
    let data_buffer = Arc::new(DataBuffer::new(&settings.symbols));
    tracing::info!(
        "Data buffer initialized for {} symbols",
        settings.symbols.len()
    );

    // Subscribe to config changes for streamer/aggregator updates
    let mut config_rx = config_reloader.subscribe();
    let _db_clone = database.clone();
    tokio::spawn(async move {
        while config_rx.changed().await.is_ok() {
            let new_settings = config_rx.borrow().clone();
            tracing::info!(
                "[CONFIG] Configuration changed, new settings: {:?}",
                new_settings
            );
            // Note: In a production app, you'd restart streamer/aggregator here
            // For now, we just log the change
        }
    });

    let streamer = Streamer::new(database.clone(), data_buffer.clone());

    tracing::info!("Starting stock stream...");
    // Streaming disabled - requires alpaca_api_client
    // streamer.start(settings.trade_symbols(), settings.bar_symbols());

    // Start aggregator
    let aggregator = Aggregator::new(database, settings.symbols.clone());
    aggregator.start();

    // Initialize trading service if enabled
    if let Some(trading_config) = &settings.trading {
        if trading_config.enabled {
            tracing::info!("[TRADING] Initializing trading system...");

            // Create broker client based on configuration
            let broker: Arc<dyn broker_client::BrokerClient> =
                match trading_config.broker.broker_type.as_str() {
                    "depot" => {
                        let depot_url = trading_config
                            .broker
                            .depot_url
                            .as_ref()
                            .expect("depot_url required for depot broker");
                        tracing::info!("[TRADING] Connecting to Depot at {}...", depot_url);
                        Arc::new(broker_client::DepotClient::connect(depot_url).await?)
                    }
                    "alpaca" => {
                        let api_key = trading_config
                            .broker
                            .alpaca_api_key
                            .as_ref()
                            .expect("alpaca_api_key required for alpaca broker");
                        let api_secret = trading_config
                            .broker
                            .alpaca_api_secret
                            .as_ref()
                            .expect("alpaca_api_secret required for alpaca broker");
                        tracing::info!(
                            "[TRADING] Connecting to Alpaca (paper: {})...",
                            trading_config.broker.alpaca_paper
                        );
                        Arc::new(broker_client::AlpacaClient::new(
                            api_key.clone(),
                            api_secret.clone(),
                            trading_config.broker.alpaca_paper,
                        ))
                    }
                    _ => {
                        anyhow::bail!("Unknown broker type: {}", trading_config.broker.broker_type);
                    }
                };

            // Create position manager
            let position_manager = position_manager::PositionManager::new(
                broker.clone(),
                trading_config.max_position_size_pct,
                trading_config.max_total_exposure_pct,
                trading_config.max_short_position_pct,
                trading_config.max_short_exposure_pct,
            );

            // Create signal aggregator
            let signal_aggregator = signal_aggregator::SignalAggregator::new(
                trading_config.buy_threshold,
                trading_config.sell_threshold,
            );

            // Create notifier
            let notifier: Arc<dyn notification::Notifier> = Arc::new(notification::LogNotifier);

            // Connect to indicator service
            tracing::info!("[TRADING] Connecting to indicator service...");
            let indicator_client = IndicatorClient::connect("http://localhost:50052").await?;

            // Create trading service
            let mut trading_service = TradingService::new(
                data_buffer.clone(),
                indicator_client,
                signal_aggregator,
                position_manager,
                notifier,
            );

            // Add strategies from configuration
            for strategy_config in &trading_config.strategies {
                match strategy_config.strategy_type.as_str() {
                    "ma_cross" => {
                        let short_period = strategy_config.params["short_period"]
                            .as_u64()
                            .unwrap_or(50) as usize;
                        let long_period = strategy_config.params["long_period"]
                            .as_u64()
                            .unwrap_or(200) as usize;
                        trading_service.add_strategy(
                            Box::new(MovingAverageCrossStrategy::new(short_period, long_period)),
                            strategy_config.weight,
                        );
                        tracing::info!(
                            "[TRADING] Added MA Cross strategy (weight: {})",
                            strategy_config.weight
                        );
                    }
                    "rsi" => {
                        let period =
                            strategy_config.params["period"].as_u64().unwrap_or(14) as usize;
                        let oversold = strategy_config.params["oversold"].as_f64().unwrap_or(30.0);
                        let overbought = strategy_config.params["overbought"]
                            .as_f64()
                            .unwrap_or(70.0);
                        trading_service.add_strategy(
                            Box::new(RsiStrategy::new(period, oversold, overbought)),
                            strategy_config.weight,
                        );
                        tracing::info!(
                            "[TRADING] Added RSI strategy (weight: {})",
                            strategy_config.weight
                        );
                    }
                    "bb" => {
                        let period =
                            strategy_config.params["period"].as_u64().unwrap_or(20) as usize;
                        let multiplier =
                            strategy_config.params["multiplier"].as_f64().unwrap_or(2.0);
                        trading_service.add_strategy(
                            Box::new(BollingerBandsStrategy::new(period, multiplier)),
                            strategy_config.weight,
                        );
                        tracing::info!(
                            "[TRADING] Added Bollinger Bands strategy (weight: {})",
                            strategy_config.weight
                        );
                    }
                    _ => {
                        tracing::warn!(
                            "[TRADING] Unknown strategy type: {}",
                            strategy_config.strategy_type
                        );
                    }
                }
            }

            // Periodically evaluate signals
            let symbols = settings.symbols.clone();
            let interval_secs = trading_config.evaluation_interval_seconds;
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
                loop {
                    interval.tick().await;
                    tracing::debug!("[TRADING] Evaluating signals...");

                    for symbol in &symbols {
                        if let Err(e) = trading_service.evaluate_and_execute(&symbol.name).await {
                            tracing::error!("[TRADING] Error evaluating {}: {:?}", symbol.name, e);
                        }
                    }
                }
            });

            tracing::info!("[TRADING] Trading system initialized and running");
        } else {
            tracing::info!("[TRADING] Trading is disabled in configuration");
        }
    } else {
        tracing::info!("[TRADING] No trading configuration found");
    }

    // Start both gRPC servers concurrently
    let config_addr = "0.0.0.0:50051".parse()?;
    let indicator_addr = "0.0.0.0:50052".parse()?;

    tracing::info!("[gRPC] Config server will listen on {}", config_addr);
    tracing::info!("[gRPC] Indicator server will listen on {}", indicator_addr);

    // Run both servers concurrently
    tokio::try_join!(
        start_grpc_server(config_reloader, config_addr),
        start_indicator_server(data_buffer, indicator_addr)
    )?;

    Ok(())
}

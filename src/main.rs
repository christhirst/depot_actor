mod aggregator;
mod data_buffer;
mod db;
mod grpc_server;
mod indicator_client;
mod indicator_server;
mod settings;
mod trader;

use tracing::info;

// #[cfg(test)]
// mod db_test; // Disabled - requires alpaca_api_client
use crate::aggregator::Aggregator;
use crate::data_buffer::DataBuffer;
use crate::db::Database;
use crate::indicator_client::IndicatorClient;
use crate::indicator_server::start_indicator_server;
use crate::settings::Settings;
use crate::trader::config_reloader::{start_config_server, ConfigGrpcService};
use crate::trader::streamer::Streamer;
use crate::trader::trading_strategy::{
    BollingerBandsStrategy, MovingAverageCrossStrategy, RsiStrategy, TradingService,
};

use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting depot_actor...");

    // Load initial settings
    let initial_settings = Settings::load()?;
    let settings = Arc::new(tokio::sync::RwLock::new(initial_settings));

    // Initialize tracing with configured log level
    let log_level = settings
        .read()
        .await
        .log_level
        .parse::<tracing::Level>()
        .unwrap_or(tracing::Level::INFO);
    println!("{log_level}");
    tracing_subscriber::fmt().with_max_level(log_level).init();
    info!("Config service created");

    //Reload channel
    let (reload_tx, mut reload_rx) = tokio::sync::broadcast::channel::<()>(16);
    // Create config service
    let config_service = ConfigGrpcService::new(settings.clone(), reload_tx.clone());
    // Start config gRPC server immediately (outside the restart loop)
    let config_grpc_port = std::env::var("CONFIG_GRPC_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(50051);
    let config_addr: std::net::SocketAddr = format!("0.0.0.0:{config_grpc_port}").parse()?;
    let indicator_addr: std::net::SocketAddr = "0.0.0.0:50052".parse()?;

    info!("[gRPC] Config server will listen on {}", config_addr);
    info!("[gRPC] Indicator server will listen on {}", indicator_addr);

    // Spawn usage of config_service before moving it
    tokio::spawn(async move {
        if let Err(e) = start_config_server(config_service, config_grpc_port).await {
            tracing::error!("Config gRPC server failed: {:?}", e);
        }
    });

    loop {
        tracing::info!("Starting / Restarting core services...");
        let mut handles = vec![];

        let db_conn_str = settings.read().await.database.connection_string();
        info!("Connecting to database...");

        let database = match Database::new(&db_conn_str).await {
            Ok(db) => {
                info!("Database initialized.");
                Arc::new(db)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize database: {:?}. Continuing without DB for tests.",
                    e
                );
                // This will cause streamers/aggregators to panic if they actually use the DB,
                // but is sufficient for testing if the reloader loops actually work.
                let db = Database::new("mysql://dummy:dummy@localhost:3306/dummy")
                    .await
                    .unwrap_or_else(|_| Database {
                        pool: sqlx::mysql::MySqlPoolOptions::new()
                            .connect_lazy("mysql://dummy:dummy@localhost:3306/dummy")
                            .unwrap(),
                    });
                Arc::new(db)
            }
        };

        // Create data buffer from settings
        let symbols = settings.read().await.symbols.clone();
        let data_buffer = Arc::new(DataBuffer::new(&symbols));

        info!("Data buffer initialized for {} symbols", symbols.len());

        // Populate the buffer with historical data
        info!("Pre-filling data buffer from database...");
        if let Err(e) = data_buffer.init_from_db(&database).await {
            tracing::error!("Failed to initialize data buffer from database: {:?}", e);
        }

        // Spawn indicator server
        let data_buffer_clone = data_buffer.clone();
        handles.push(tokio::spawn(async move {
            if let Err(e) = start_indicator_server(data_buffer_clone, indicator_addr).await {
                tracing::error!("Indicator gRPC server failed: {:?}", e);
            }
        }));

        // Give servers a moment to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        let trading_config = {
            let s = settings.read().await;
            s.trading.clone()
        };

        if let Some(ref config) = trading_config {
            if let Some(key) = &config.broker.alpaca_api_key {
                std::env::set_var("APCA_API_KEY_ID", key);
            }
            if let Some(secret) = &config.broker.alpaca_api_secret {
                std::env::set_var("APCA_API_SECRET_KEY", secret);
            }
        }

        let streamer = Streamer::new(database.clone(), data_buffer.clone());

        info!("Starting stock stream...");
        let trade_symbols = {
            let s = settings.read().await;
            s.trade_symbols()
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        let bar_symbols = {
            let s = settings.read().await;
            s.bar_symbols()
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        tokio::task::spawn_blocking(move || {
            let t_refs: Vec<&str> = trade_symbols.iter().map(|s| s.as_str()).collect();
            let b_refs: Vec<&str> = bar_symbols.iter().map(|s| s.as_str()).collect();
            streamer.start(t_refs, b_refs);
        });

        // Start aggregator
        let aggregator = Aggregator::new(database, symbols.clone());
        let mut agg_handles = aggregator.start();
        handles.append(&mut agg_handles);

        // Initialize trading service if enabled
        let trading_config = {
            let s = settings.read().await;
            s.trading.clone()
        };

        if let Some(trading_config) = trading_config {
            if trading_config.enabled {
                tracing::info!("[TRADING] Initializing trading system...");

                // Create broker client based on configuration
                let broker = match trading_config.broker.broker_type.as_str() {
                    "depot" => match trading_config.broker.depot_url.as_ref() {
                        Some(depot_url) => {
                            println!("[TRADING] Connecting to Depot at {}...", depot_url);
                            tracing::info!("[TRADING] Connecting to Depot at {}...", depot_url);
                            match trader::broker_client::DepotClient::connect(depot_url).await {
                                Ok(client) => Some(Arc::new(client)
                                    as Arc<dyn trader::broker_client::BrokerClient>),
                                Err(e) => {
                                    tracing::error!(
                                        "[TRADING] Failed to connect to Depot at {}: {:?}",
                                        depot_url,
                                        e
                                    );
                                    tracing::warn!(
                                        "[TRADING] Trading disabled for this run; core services stay online"
                                    );
                                    None
                                }
                            }
                        }
                        None => {
                            tracing::error!(
                                "[TRADING] depot_url is required when broker_type is depot"
                            );
                            None
                        }
                    },
                    "alpaca" => {
                        match (
                            trading_config.broker.alpaca_api_key.as_ref(),
                            trading_config.broker.alpaca_api_secret.as_ref(),
                        ) {
                            (Some(api_key), Some(api_secret)) => {
                                println!(
                                    "[TRADING] Connecting to Alpaca (paper: {})...",
                                    trading_config.broker.alpaca_paper
                                );
                                tracing::info!(
                                    "[TRADING] Connecting to Alpaca (paper: {})...",
                                    trading_config.broker.alpaca_paper
                                );
                                Some(Arc::new(trader::broker_client::AlpacaClient::new(
                                    api_key.clone(),
                                    api_secret.clone(),
                                    trading_config.broker.alpaca_paper,
                                ))
                                    as Arc<dyn trader::broker_client::BrokerClient>)
                            }
                            _ => {
                                tracing::error!(
                                "[TRADING] alpaca_api_key and alpaca_api_secret are required when broker_type is alpaca"
                            );
                                None
                            }
                        }
                    }
                    _ => {
                        tracing::error!(
                            "[TRADING] Unknown broker type: {}",
                            trading_config.broker.broker_type
                        );
                        None
                    }
                };
                if let Some(broker) = broker {
                    // Create position manager
                    let position_manager = trader::position_manager::PositionManager::new(
                        broker.clone(),
                        trading_config.max_position_size_pct,
                        trading_config.max_total_exposure_pct,
                        trading_config.max_short_position_pct,
                        trading_config.max_short_exposure_pct,
                    );

                    // Create signal aggregator
                    let signal_aggregator = trader::signal_aggregator::SignalAggregator::new(
                        trading_config.buy_threshold,
                        trading_config.sell_threshold,
                    );

                    // Create notifier
                    let notifier: Arc<dyn trader::notification::Notifier> =
                        Arc::new(trader::notification::LogNotifier);

                    // Connect to indicator service
                    println!("[TRADING] Connecting to indicator service...");
                    tracing::info!("[TRADING] Connecting to indicator service...");

                    let indicator_client =
                        match IndicatorClient::connect("http://127.0.0.1:50052").await {
                            Ok(client) => Some(client),
                            Err(e) => {
                                tracing::error!(
                                    "[TRADING] Failed to connect to indicator service: {:?}",
                                    e
                                );
                                tracing::warn!(
                        "[TRADING] Trading disabled for this run; core services stay online"
                    );
                                None
                            }
                        };

                    if let Some(indicator_client) = indicator_client {
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
                                        .unwrap_or(50)
                                        as usize;
                                    let long_period = strategy_config.params["long_period"]
                                        .as_u64()
                                        .unwrap_or(200)
                                        as usize;
                                    trading_service.add_strategy(
                                        Box::new(MovingAverageCrossStrategy::new(
                                            short_period,
                                            long_period,
                                        )),
                                        strategy_config.weight,
                                    );
                                    tracing::info!(
                                        "[TRADING] Added MA Cross strategy (weight: {})",
                                        strategy_config.weight
                                    );
                                }
                                "rsi" => {
                                    let period =
                                        strategy_config.params["period"].as_u64().unwrap_or(14)
                                            as usize;
                                    let oversold =
                                        strategy_config.params["oversold"].as_f64().unwrap_or(30.0);
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
                                        strategy_config.params["period"].as_u64().unwrap_or(20)
                                            as usize;
                                    let multiplier = strategy_config.params["multiplier"]
                                        .as_f64()
                                        .unwrap_or(2.0);
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
                        let symbols = symbols.clone();
                        let interval_secs = trading_config.evaluation_interval_seconds;
                        handles.push(tokio::spawn(async move {
                            let mut interval =
                                tokio::time::interval(Duration::from_secs(interval_secs));
                            loop {
                                interval.tick().await;
                                tracing::debug!("[TRADING] Evaluating signals...");

                                for symbol in &symbols {
                                    if let Err(e) =
                                        trading_service.evaluate_and_execute(&symbol.name).await
                                    {
                                        tracing::error!(
                                            "[TRADING] Error evaluating {}: {:?}",
                                            symbol.name,
                                            e
                                        );
                                    }
                                }
                            }
                        }));

                        tracing::info!("[TRADING] Trading system initialized and running");
                    } else {
                        tracing::warn!(
                            "[TRADING] Trading initialization skipped; service will continue running"
                        );
                    }
                }
            } else {
                tracing::info!("[TRADING] Trading is disabled in configuration");
            }
        } else {
            tracing::info!("[TRADING] No trading configuration found");
        }

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down gracefully...");
                for handle in handles {
                    handle.abort();
                }
                break;
            }
            _ = reload_rx.recv() => {
                info!("Configuration updated! Restarting services...");
                for handle in handles {
                    handle.abort();
                }
                // Wait briefly for tasks to abort
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        }
    }

    Ok(())
}

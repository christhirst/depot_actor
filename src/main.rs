mod aggregator;
mod config_reloader;
mod db;
#[cfg(test)]
mod db_test;
mod grpc_server;
mod settings;
mod streamer;

use crate::aggregator::Aggregator;
use crate::config_reloader::ConfigReloader;
use crate::db::Database;
use crate::grpc_server::start_grpc_server;
use crate::settings::Settings;
use crate::streamer::Streamer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting depot_actor...");

    // Create config reloader
    let config_reloader =
        ConfigReloader::new(Settings::load).expect("Failed to load initial configuration");

    let settings = config_reloader.get();
    println!("Settings loaded: {:?}", settings);

    let db_conn_str = settings.database.connection_string();
    println!("Connecting to database...");

    let database = Database::new(&db_conn_str).await?;
    let database = Arc::new(database);
    println!("Database initialized.");

    // Subscribe to config changes for streamer/aggregator updates
    let mut config_rx = config_reloader.subscribe();
    let db_clone = database.clone();
    tokio::spawn(async move {
        while config_rx.changed().await.is_ok() {
            let new_settings = config_rx.borrow().clone();
            println!(
                "[CONFIG] Configuration changed, new settings: {:?}",
                new_settings
            );
            // Note: In a production app, you'd restart streamer/aggregator here
            // For now, we just log the change
        }
    });

    let streamer = Streamer::new(database.clone());

    println!("Starting stock stream...");
    streamer.start(settings.trade_symbols(), settings.bar_symbols());

    // Start aggregator
    let aggregator = Aggregator::new(database, settings.symbols.clone());
    aggregator.start();

    // Start gRPC server for config reload
    let grpc_addr = "0.0.0.0:50051".parse()?;
    println!("[gRPC] Server will listen on {}", grpc_addr);

    // Run gRPC server (this blocks)
    start_grpc_server(config_reloader, grpc_addr).await?;

    Ok(())
}

mod db;
#[cfg(test)]
mod db_test;
mod settings;
mod streamer;

use crate::db::Database;
use crate::settings::Settings;
use crate::streamer::Streamer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting depot_actor...");

    let settings = Settings::load().expect("Failed to load configuration");
    println!("Settings loaded: {:?}", settings);

    let db_conn_str = settings.database.connection_string();
    println!("Connecting to database...");

    let database = Database::new(&db_conn_str).await?;
    let database = Arc::new(database);
    println!("Database initialized.");

    let streamer = Streamer::new(database);

    println!("Starting stock stream...");
    streamer.start(settings.trade_symbols, settings.bar_symbols);

    Ok(())
}

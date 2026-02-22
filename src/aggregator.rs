use crate::db::Database;
use crate::settings::SymbolConfig;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::info;

pub struct Aggregator {
    db: Arc<Database>,
    symbols: Vec<SymbolConfig>,
}

impl Aggregator {
    pub fn new(db: Arc<Database>, symbols: Vec<SymbolConfig>) -> Self {
        Self { db, symbols }
    }

    pub fn start(self) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = Vec::new();

        // Spawn a task for each symbol with its own interval
        for symbol_config in self.symbols {
            let db = self.db.clone();
            let symbol = symbol_config.name.clone();
            let interval_hours = std::cmp::max(1, symbol_config.aggregation_interval_hours);

            handles.push(tokio::spawn(async move {
                let mut ticker = interval(Duration::from_secs((interval_hours as u64) * 3600));

                loop {
                    ticker.tick().await;

                    info!(
                        "[AGGREGATOR] Running aggregation for {} (interval: {}h)",
                        symbol, interval_hours
                    );

                    // Aggregate trades
                    if let Err(e) = db.aggregate_trades(&symbol, interval_hours).await {
                        info!(
                            "[AGGREGATOR] Error aggregating trades for {}: {:?}",
                            symbol, e
                        );
                    } else {
                        info!("[AGGREGATOR] Successfully aggregated trades for {}", symbol);
                    }

                    // Aggregate bars
                    if let Err(e) = db.aggregate_bars(&symbol, interval_hours).await {
                        info!(
                            "[AGGREGATOR] Error aggregating bars for {}: {:?}",
                            symbol, e
                        );
                    } else {
                        info!("[AGGREGATOR] Successfully aggregated bars for {}", symbol);
                    }
                }
            }));
        }

        info!("[AGGREGATOR] Started aggregation tasks for all symbols");
        handles
    }
}

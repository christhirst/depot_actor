use crate::data_buffer::DataBuffer;
use crate::db::Database;
use alpaca_api_client::{Feed, MarketDataMessage, StockStream};
use std::sync::Arc;
use tokio::runtime::Handle;
use tracing::{error, info};

pub struct Streamer {
    db: Arc<Database>,
    data_buffer: Arc<DataBuffer>,
}

impl Streamer {
    pub fn new(db: Arc<Database>, data_buffer: Arc<DataBuffer>) -> Self {
        Self { db, data_buffer }
    }

    pub fn start(&self, trade_symbols: Vec<&str>, bar_symbols: Vec<&str>) {
        let trade_refs: Vec<&str> = trade_symbols.iter().copied().collect();
        let bar_refs: Vec<&str> = bar_symbols.iter().copied().collect();

        let db = self.db.clone();
        let data_buffer = self.data_buffer.clone();
        let rt = Handle::current();

        let stream_result = StockStream::new(Feed::Test)
            .subscribe_trades(trade_refs)
            .subscribe_bars(bar_refs)
            .start(move |msg| {
                // Extract price and add to buffer
                match &msg {
                    MarketDataMessage::Trade(t) => {
                        info!("[TRADE] {} ${} x{}", t.symbol, t.p, t.s);
                        data_buffer.add_price(&t.symbol, t.p);
                    }
                    MarketDataMessage::Bar(b) => {
                        info!("[BAR] {} C={}", b.symbol, b.c);
                        data_buffer.add_price(&b.symbol, b.c as f64);
                    }
                    _ => {}
                }

                // Spawning async save task to not block the stream callback
                let db_clone = db.clone();
                rt.spawn(async move {
                    if let Err(e) = db_clone.save_message(msg).await {
                        error!("Error saving message to DB: {:?}", e);
                    }
                });
            });

        if let Err(e) = stream_result {
            error!(
                "Alpaca stream failed to start or connection closed: {:?}",
                e
            );
        }
    }
}

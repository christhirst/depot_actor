use crate::db::Database;
use alpaca_api_client::{Feed, MarketDataMessage, StockStream};
use std::sync::Arc;
use tokio::runtime::Handle;

pub struct Streamer {
    db: Arc<Database>,
}

impl Streamer {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn start(&self, trade_symbols: Vec<String>, bar_symbols: Vec<String>) {
        let trade_refs: Vec<&str> = trade_symbols.iter().map(|s| s.as_str()).collect();
        let bar_refs: Vec<&str> = bar_symbols.iter().map(|s| s.as_str()).collect();

        let db = self.db.clone();
        let rt = Handle::current();

        StockStream::new(Feed::Test)
            .subscribe_trades(trade_refs)
            .subscribe_bars(bar_refs)
            .start(move |msg| {
                match &msg {
                    MarketDataMessage::Trade(t) => {
                        println!("[TRADE] {} ${} x{}", t.symbol, t.p, t.s)
                    }
                    MarketDataMessage::Bar(b) => println!("[BAR] {} C={}", b.symbol, b.c),
                    _ => {}
                }

                // Spawning async save task to not block the stream callback
                let db_clone = db.clone();
                rt.spawn(async move {
                    if let Err(e) = db_clone.save_message(msg).await {
                        eprintln!("Error saving message to DB: {:?}", e);
                    }
                });
            })
            .unwrap();
    }
}

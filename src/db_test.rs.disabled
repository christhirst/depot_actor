use crate::db::Database;
use crate::settings::Settings;
use alpaca_api_client::{MarketDataMessage, StreamBar, StreamTrade};

async fn setup_db() -> Database {
    let settings = Settings::load().expect("Failed to load settings for test");
    let conn_str = settings.database.connection_string();

    Database::new(&conn_str)
        .await
        .expect("Failed to connect to test database. Please ensure MariaDB is running and credentials are correct.")
}

#[tokio::test]
async fn test_save_message_trade() {
    let db = setup_db().await;
    // Clear potentially existing data
    db.drop_tables()
        .await
        .expect("Failed to drop tables before test");
    // Re-initialize tables
    let settings = Settings::load().expect("Failed to load settings");
    let conn_str = settings.database.connection_string();
    let db = Database::new(&conn_str).await.expect("Failed to reconnect");

    let trade = StreamTrade {
        msg_type: "t".to_string(),
        symbol: "TEST_TRADE".to_string(),
        p: 155.5,
        s: 100.0,
        t: "2023-01-01T00:00:00Z".to_string(),
        i: Some(1),
        x: Some("V".to_string()),
        z: Some("C".to_string()),
        c: Some(vec![]),
    };

    let msg = MarketDataMessage::Trade(trade);
    db.save_message(msg)
        .await
        .expect("Failed to save trade message");

    let trades = db
        .get_trades("TEST_TRADE")
        .await
        .expect("Failed to get trades");
    assert!(!trades.is_empty());
    assert_eq!(trades[0].0, "TEST_TRADE");
    assert_eq!(trades[0].1, 155.5);
    assert_eq!(trades[0].2, 100);
}

#[tokio::test]
async fn test_save_message_bar() {
    let db = setup_db().await;
    // Cleanup
    let _ = sqlx::query("DELETE FROM bars WHERE symbol = 'TEST_BAR'")
        .execute(&db.pool)
        .await;

    let bar = StreamBar {
        bar_type: "b".to_string(),
        symbol: "TEST_BAR".to_string(),
        o: 100.0,
        h: 105.0,
        l: 95.0,
        c: 102.0,
        v: 1000,
        t: "2023-01-01T00:00:00Z".to_string(),
        n: 100,
        vw: 101.5,
    };

    let msg = MarketDataMessage::Bar(bar);
    db.save_message(msg)
        .await
        .expect("Failed to save bar message");

    let bars = db.get_bars("TEST_BAR").await.expect("Failed to get bars");
    assert!(!bars.is_empty());
    let b = &bars[0];
    assert_eq!(b.0, "TEST_BAR");
    assert_eq!(b.1, 100.0); // Open
    assert_eq!(b.2, 105.0); // High
    assert_eq!(b.3, 95.0); // Low
    assert_eq!(b.4, 102.0); // Close
    assert_eq!(b.5, 1000); // Volume
}

#[tokio::test]
async fn test_get_trades() {
    let db = setup_db().await;
    // Cleanup
    let _ = sqlx::query("DELETE FROM trades WHERE symbol = 'TEST_GET_TRADES'")
        .execute(&db.pool)
        .await;

    sqlx::query("INSERT INTO trades (symbol, price, size) VALUES (?, ?, ?)")
        .bind("TEST_GET_TRADES")
        .bind(200.0)
        .bind(50)
        .execute(&db.pool)
        .await
        .expect("Failed to insert test trade");

    let trades = db
        .get_trades("TEST_GET_TRADES")
        .await
        .expect("Failed to get trades");
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].0, "TEST_GET_TRADES");
    assert_eq!(trades[0].1, 200.0);
}

#[tokio::test]
async fn test_get_bars() {
    let db = setup_db().await;
    // Cleanup
    let _ = sqlx::query("DELETE FROM bars WHERE symbol = 'TEST_GET_BARS'")
        .execute(&db.pool)
        .await;

    sqlx::query(
        "INSERT INTO bars (symbol, open, high, low, close, volume) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("TEST_GET_BARS")
    .bind(50.0)
    .bind(55.0)
    .bind(45.0)
    .bind(52.0)
    .bind(500)
    .execute(&db.pool)
    .await
    .expect("Failed to insert test bar");

    let bars = db
        .get_bars("TEST_GET_BARS")
        .await
        .expect("Failed to get bars");
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].1, 50.0);
    assert_eq!(bars[0].5, 500);
}

#[tokio::test]
async fn test_drop_tables() {
    let db = setup_db().await;

    // Ensure tables exist
    let settings = Settings::load().expect("Failed to load settings");
    let conn_str = settings.database.connection_string();
    let _ = Database::new(&conn_str)
        .await
        .expect("Failed to init tables");

    // Now drop them
    db.drop_tables().await.expect("Failed to drop tables");

    // Verify trades table is gone
    let result = sqlx::query("SELECT * FROM trades LIMIT 1")
        .execute(&db.pool)
        .await;

    // sqlx might return error if table doesn't exist
    assert!(result.is_err(), "Trades table should not exist");

    // Verify bars table is gone
    let result = sqlx::query("SELECT * FROM bars LIMIT 1")
        .execute(&db.pool)
        .await;

    assert!(result.is_err(), "Bars table should not exist");

    // Restore tables
    let _ = Database::new(&conn_str)
        .await
        .expect("Failed to recreate tables");
}

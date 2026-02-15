// use alpaca_api_client::MarketDataMessage;
use anyhow::Result;
use sqlx::MySqlPool;

pub struct Database {
    pub(crate) pool: MySqlPool,
}

impl Database {
    pub async fn init(
        admin_settings: &crate::settings::DatabaseSettings,
        app_settings: &crate::settings::DatabaseSettings,
    ) -> Result<()> {
        use sqlx::ConnectOptions;

        // 1. Connect as Admin (Root)
        let admin_conn_str = admin_settings.connection_string();
        let mut options: sqlx::mysql::MySqlConnectOptions = admin_conn_str.parse()?;
        options = options.log_statements(tracing::log::LevelFilter::Debug);

        // We need to connect to the server (mysql system db) to manage users,
        // effectively ignoring the database name in admin_settings for the user creation part
        // but we need it for GRANT.
        // Actually, let's just connect to 'mysql' first.
        let mut server_options = options.clone();
        server_options = server_options.database("mysql");

        let admin_pool = MySqlPool::connect_with(server_options).await?;

        // 2. Create the App User
        // Note: We use the host from app_settings to define where the user can connect from (usually '%')
        // But for simplicity and the requirement, let's assume '%' or the specific host.
        // The user said: "Create the user with a init() method".
        // We'll create user@'%' to allow access from anywhere (common in containerized envs)
        // or user@'172.18.0.1' (gateway). Safer to use '%' for this setup.

        let app_user = &app_settings.user;
        let app_pass = &app_settings.password;
        let db_name = &app_settings.database;

        // Ensure the database exists (using admin creds)
        sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name))
            .execute(&admin_pool)
            .await?;

        // Create User
        // We use query! macro or format! generic query because placeholders don't work for identifiers
        sqlx::query(&format!(
            "CREATE USER IF NOT EXISTS '{}'@'%' IDENTIFIED BY '{}'",
            app_user, app_pass
        ))
        .execute(&admin_pool)
        .await?;

        // 3. Grant Privileges
        sqlx::query(&format!(
            "GRANT ALL PRIVILEGES ON `{}`.* TO '{}'@'%'",
            db_name, app_user
        ))
        .execute(&admin_pool)
        .await?;

        // 4. Flush Privileges
        sqlx::query("FLUSH PRIVILEGES").execute(&admin_pool).await?;

        Ok(())
    }

    pub async fn new(connection_string: &str) -> Result<Self> {
        use sqlx::ConnectOptions;
        let mut options: sqlx::mysql::MySqlConnectOptions = connection_string.parse()?;
        options = options.log_statements(tracing::log::LevelFilter::Debug);

        if let Some(db_name) = options.get_database() {
            let db_name = db_name.to_string();

            // Connect to server without target DB to create it if it doesn't exist
            let mut server_options = options.clone();
            server_options = server_options.database("mysql");

            if let Ok(server_pool) = MySqlPool::connect_with(server_options).await {
                let _ = sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name))
                    .execute(&server_pool)
                    .await;
                server_pool.close().await;
            }
        }

        let pool = MySqlPool::connect_with(options).await?;

        // Initialize tables
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS trades (
                id INT AUTO_INCREMENT PRIMARY KEY,
                symbol VARCHAR(16),
                price DECIMAL(16, 4),
                size INT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bars (
                id INT AUTO_INCREMENT PRIMARY KEY,
                symbol VARCHAR(16),
                open DECIMAL(16, 4),
                high DECIMAL(16, 4),
                low DECIMAL(16, 4),
                close DECIMAL(16, 4),
                volume BIGINT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        // Create aggregated data tables
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS aggregated_trades (
                id INT AUTO_INCREMENT PRIMARY KEY,
                symbol VARCHAR(16),
                date DATE,
                avg_price DECIMAL(16, 4),
                total_volume BIGINT,
                trade_count INT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE KEY unique_symbol_date (symbol, date)
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS aggregated_bars (
                id INT AUTO_INCREMENT PRIMARY KEY,
                symbol VARCHAR(16),
                date DATE,
                open DECIMAL(16, 4),
                high DECIMAL(16, 4),
                low DECIMAL(16, 4),
                close DECIMAL(16, 4),
                volume BIGINT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE KEY unique_symbol_date (symbol, date)
            )",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /* Commented out - requires alpaca_api_client for streaming
    pub async fn save_message(&self, msg: MarketDataMessage) -> Result<()> {
        match msg {
            MarketDataMessage::Trade(t) => {
                sqlx::query("INSERT INTO trades (symbol, price, size) VALUES (?, ?, ?)")
                    .bind(&t.symbol)
                    .bind(t.p)
                    .bind(t.s as i32)
                    .execute(&self.pool)
                    .await?;
            }
            MarketDataMessage::Bar(b) => {
                sqlx::query("INSERT INTO bars (symbol, open, high, low, close, volume) VALUES (?, ?, ?, ?, ?, ?)")
                    .bind(&b.symbol)
                    .bind(b.o)
                    .bind(b.h)
                    .bind(b.l)
                    .bind(b.c)
                    .bind(b.v as i64)
                    .execute(&self.pool)
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
    */

    pub async fn get_trades(&self, symbol: &str) -> Result<Vec<(String, f64, i32)>> {
        let trades = sqlx::query_as::<_, (String, f64, i32)>(
            "SELECT symbol, CAST(price AS DOUBLE), size FROM trades WHERE symbol = ?",
        )
        .bind(symbol)
        .fetch_all(&self.pool)
        .await?;
        Ok(trades)
    }

    pub async fn get_bars(&self, symbol: &str) -> Result<Vec<(String, f64, f64, f64, f64, i64)>> {
        let bars = sqlx::query_as::<_, (String, f64, f64, f64, f64, i64)>(
            "SELECT symbol, CAST(open AS DOUBLE), CAST(high AS DOUBLE), CAST(low AS DOUBLE), CAST(close AS DOUBLE), volume FROM bars WHERE symbol = ?"
        )
        .bind(symbol)
        .fetch_all(&self.pool)
        .await?;
        Ok(bars)
    }

    pub async fn drop_tables(&self) -> Result<()> {
        sqlx::query("DROP TABLE IF EXISTS trades")
            .execute(&self.pool)
            .await?;
        sqlx::query("DROP TABLE IF EXISTS bars")
            .execute(&self.pool)
            .await?;
        sqlx::query("DROP TABLE IF EXISTS aggregated_trades")
            .execute(&self.pool)
            .await?;
        sqlx::query("DROP TABLE IF EXISTS aggregated_bars")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Aggregate trades for a symbol over the specified interval
    pub async fn aggregate_trades(&self, symbol: &str, interval_hours: u32) -> Result<()> {
        let query = format!(
            "INSERT INTO aggregated_trades (symbol, date, avg_price, total_volume, trade_count)
             SELECT 
                 symbol,
                 DATE(timestamp) as date,
                 AVG(price) as avg_price,
                 SUM(size) as total_volume,
                 COUNT(*) as trade_count
             FROM trades
             WHERE symbol = ?
                 AND timestamp >= DATE_SUB(NOW(), INTERVAL {} HOUR)
                 AND DATE(timestamp) NOT IN (
                     SELECT date FROM aggregated_trades WHERE symbol = ?
                 )
             GROUP BY symbol, DATE(timestamp)
             ON DUPLICATE KEY UPDATE
                 avg_price = VALUES(avg_price),
                 total_volume = VALUES(total_volume),
                 trade_count = VALUES(trade_count)",
            interval_hours
        );

        sqlx::query(&query)
            .bind(symbol)
            .bind(symbol)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Aggregate bars for a symbol over the specified interval
    pub async fn aggregate_bars(&self, symbol: &str, interval_hours: u32) -> Result<()> {
        let query = format!(
            "INSERT INTO aggregated_bars (symbol, date, open, high, low, close, volume)
             SELECT 
                 symbol,
                 DATE(timestamp) as date,
                 SUBSTRING_INDEX(GROUP_CONCAT(open ORDER BY timestamp ASC), ',', 1) as open,
                 MAX(high) as high,
                 MIN(low) as low,
                 SUBSTRING_INDEX(GROUP_CONCAT(close ORDER BY timestamp DESC), ',', 1) as close,
                 SUM(volume) as volume
             FROM bars
             WHERE symbol = ?
                 AND timestamp >= DATE_SUB(NOW(), INTERVAL {} HOUR)
                 AND DATE(timestamp) NOT IN (
                     SELECT date FROM aggregated_bars WHERE symbol = ?
                 )
             GROUP BY symbol, DATE(timestamp)
             ON DUPLICATE KEY UPDATE
                 open = VALUES(open),
                 high = VALUES(high),
                 low = VALUES(low),
                 close = VALUES(close),
                 volume = VALUES(volume)",
            interval_hours
        );

        sqlx::query(&query)
            .bind(symbol)
            .bind(symbol)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    async fn setup_db() -> Database {
        let settings = Settings::load().expect("Failed to load settings for test");
        let conn_str = settings.database.connection_string();

        Database::new(&conn_str)
            .await
            .expect("Failed to connect to test database. Please ensure MariaDB is running and credentials are correct in config.toml or provided via environment variable (e.g. APP_DATABASE_PASSWORD).")
    }

    #[tokio::test]
    async fn test_database_init() {
        use crate::settings::DatabaseSettings;
        // Load settings
        let settings = Settings::load().expect("Failed to load settings");

        let app_settings = &settings.database;

        // Construct admin settings from userroot/root_password
        // We clone app_settings and override user/password
        let admin_settings = DatabaseSettings {
            host: app_settings.host.clone(),
            port: app_settings.port,
            user: app_settings
                .userroot
                .clone()
                .unwrap_or_else(|| "root".to_string()),
            password: app_settings.rootpassword.clone().unwrap_or_default(),
            database: app_settings.database.clone(),
            userroot: None,
            rootpassword: None,
        };

        // Run init
        Database::init(&admin_settings, app_settings)
            .await
            .expect("Failed to initialize database user");

        // Verify connection with app user
        let conn_str = app_settings.connection_string();
        let db = Database::new(&conn_str)
            .await
            .expect("Failed to connect with new user");

        // Verify permissions (Create table, insert, select)
        // Note: Database::new already creates tables, so if we are here, CREATE TABLE worked (or IF EXISTS worked)
        // Let's try to insert data
        sqlx::query("INSERT INTO trades (symbol, price, size) VALUES (?, ?, ?)")
            .bind("TEST_USER_INIT")
            .bind(100.0)
            .bind(10)
            .execute(&db.pool)
            .await
            .expect("Failed to insert with new user");

        // Clean up (optional, but good practice)
        // Note: dbuser might not have DROP TABLE permission if we only granted typical app permissions.
        // But we granted ALL PRIVILEGES in init(), so it should work.
        db.drop_tables()
            .await
            .expect("Failed to drop tables with new user");
    }

    #[tokio::test]
    async fn test_database_lifecycle() {
        // This test requires a valid MariaDB connection as configured in config.toml
        let db = setup_db().await;

        // Ensure tables exist (Database::new already calls CREATE TABLE IF NOT EXISTS)

        // 1. Add Data (Manual SQL insertion to verify get_trades/get_bars)
        sqlx::query("INSERT INTO trades (symbol, price, size) VALUES (?, ?, ?)")
            .bind("TEST_UNIT")
            .bind(150.5f64)
            .bind(100i32)
            .execute(&db.pool)
            .await
            .expect("Failed to insert trade");

        // 2. Search Data (Verify get_trades)
        let trades = db
            .get_trades("TEST_UNIT")
            .await
            .expect("Failed to get trades");
        assert!(trades.len() >= 1);
        let test_trade = trades
            .iter()
            .find(|t| t.0 == "TEST_UNIT")
            .expect("Should find test trade");
        assert_eq!(test_trade.1, 150.5);
        assert_eq!(test_trade.2, 100);

        // 3. Add Data (Bar)
        sqlx::query(
            "INSERT INTO bars (symbol, open, high, low, close, volume) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("TEST_UNIT")
        .bind(100.0f64)
        .bind(110.0f64)
        .bind(95.0f64)
        .bind(105.0f64)
        .bind(1000i64)
        .execute(&db.pool)
        .await
        .expect("Failed to insert bar");

        // 4. Search Data (Verify get_bars)
        let bars = db.get_bars("TEST_UNIT").await.expect("Failed to get bars");
        assert!(bars.len() >= 1);
        let test_bar = bars
            .iter()
            .find(|b| b.0 == "TEST_UNIT")
            .expect("Should find test bar");
        assert_eq!(test_bar.5, 1000);

        // 5. Cleanup
        //db.drop_tables().await.expect("Failed to drop tables");
    }
}

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct DatabaseSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub userroot: Option<String>,
    pub rootpassword: Option<String>,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.database
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub trade_symbols: Vec<String>,
    pub bar_symbols: Vec<String>,
    pub database: DatabaseSettings,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        let s = Config::builder()
            // Start with default config file
            .add_source(File::with_name("config").required(false))
            // Override with environment variables (e.g., APP_DATABASE_PASSWORD)
            .add_source(config::Environment::with_prefix("APP").separator("_"))
            .build()?;

        s.try_deserialize()
    }
}

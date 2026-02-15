use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct SymbolConfig {
    pub name: String,
    pub subscribe_trades: bool,
    pub subscribe_bars: bool,
    pub aggregation_interval_hours: u32,
}

impl SymbolConfig {
    /// Create from proto SymbolConfig
    pub fn from_proto(proto: crate::grpc_server::config_proto::SymbolConfig) -> Self {
        SymbolConfig {
            name: proto.name,
            subscribe_trades: proto.subscribe_trades,
            subscribe_bars: proto.subscribe_bars,
            aggregation_interval_hours: proto.aggregation_interval_hours,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
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

    /// Create from proto DatabaseConfig
    pub fn from_proto(proto: crate::grpc_server::config_proto::DatabaseConfig) -> Self {
        DatabaseSettings {
            host: proto.host,
            port: proto.port as u16,
            user: proto.user,
            password: proto.password,
            database: proto.database,
            userroot: proto.userroot,
            rootpassword: proto.rootpassword,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub symbols: Vec<SymbolConfig>,
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

    /// Create from proto UpdateConfigRequest
    pub fn from_proto(proto: crate::grpc_server::config_proto::UpdateConfigRequest) -> Self {
        Settings {
            symbols: proto
                .symbols
                .into_iter()
                .map(SymbolConfig::from_proto)
                .collect(),
            database: DatabaseSettings::from_proto(
                proto.database.expect("database field is required"),
            ),
        }
    }

    /// Get symbols that should subscribe to trades
    pub fn trade_symbols(&self) -> Vec<&str> {
        self.symbols
            .iter()
            .filter(|s| s.subscribe_trades)
            .map(|s| s.name.as_str())
            .collect()
    }

    /// Get symbols that should subscribe to bars
    pub fn bar_symbols(&self) -> Vec<&str> {
        self.symbols
            .iter()
            .filter(|s| s.subscribe_bars)
            .map(|s| s.name.as_str())
            .collect()
    }
}

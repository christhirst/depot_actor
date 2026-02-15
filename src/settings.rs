use config::{Config, ConfigError, File};
use serde::Deserialize;

// Import proto types for conversion methods
use crate::grpc_server::config_proto;

#[derive(Debug, Deserialize, Clone)]
pub struct SymbolConfig {
    pub name: String,
    pub subscribe_trades: bool,
    pub subscribe_bars: bool,
    pub aggregation_interval_hours: u32,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

fn default_buffer_size() -> usize {
    200
}

impl SymbolConfig {
    /// Create from proto SymbolConfig
    pub fn from_proto(proto: config_proto::SymbolConfig) -> Self {
        SymbolConfig {
            name: proto.name,
            subscribe_trades: proto.subscribe_trades,
            subscribe_bars: proto.subscribe_bars,
            aggregation_interval_hours: proto.aggregation_interval_hours,
            buffer_size: default_buffer_size(),
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
    pub fn from_proto(proto: config_proto::DatabaseConfig) -> Self {
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
pub struct TradingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_evaluation_interval")]
    pub evaluation_interval_seconds: u64,
    #[serde(default = "default_max_position_size_pct")]
    pub max_position_size_pct: f64,
    #[serde(default = "default_max_total_exposure_pct")]
    pub max_total_exposure_pct: f64,
    #[serde(default = "default_max_short_position_pct")]
    pub max_short_position_pct: f64,
    #[serde(default = "default_max_short_exposure_pct")]
    pub max_short_exposure_pct: f64,
    #[serde(default = "default_buy_threshold")]
    pub buy_threshold: f64,
    #[serde(default = "default_sell_threshold")]
    pub sell_threshold: f64,
    pub broker: BrokerConfig,
    #[serde(default)]
    pub strategies: Vec<StrategyConfig>,
}

fn default_evaluation_interval() -> u64 {
    60
}

fn default_max_position_size_pct() -> f64 {
    0.10
}

fn default_max_total_exposure_pct() -> f64 {
    0.80
}

fn default_max_short_position_pct() -> f64 {
    0.05
}

fn default_max_short_exposure_pct() -> f64 {
    0.50
}

fn default_buy_threshold() -> f64 {
    0.6
}

fn default_sell_threshold() -> f64 {
    0.6
}

#[derive(Debug, Deserialize, Clone)]
pub struct BrokerConfig {
    pub broker_type: String, // "depot" or "alpaca"
    pub depot_url: Option<String>,
    pub alpaca_api_key: Option<String>,
    pub alpaca_api_secret: Option<String>,
    #[serde(default)]
    pub alpaca_paper: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    pub strategy_type: String, // "ma_cross", "rsi", "bb"
    pub weight: f64,
    pub params: serde_json::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub symbols: Vec<SymbolConfig>,
    pub database: DatabaseSettings,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub trading: Option<TradingConfig>,
}

fn default_log_level() -> String {
    "warn".to_string()
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
    pub fn from_proto(proto: config_proto::UpdateConfigRequest) -> Self {
        Settings {
            symbols: proto
                .symbols
                .into_iter()
                .map(SymbolConfig::from_proto)
                .collect(),
            database: DatabaseSettings::from_proto(
                proto.database.expect("database field is required"),
            ),
            log_level: default_log_level(),
            trading: None,
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

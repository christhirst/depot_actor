use anyhow::Result;
use async_trait::async_trait;
use tonic::transport::Channel;
use tracing::info;

/// Position type (long or short)
#[derive(Debug, Clone, PartialEq)]
pub enum PositionType {
    Long,  // Own shares, profit when price rises
    Short, // Borrowed shares, profit when price falls
}

/// Represents a trading position
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
    pub current_price: f64,
    pub position_type: PositionType,
}

/// Represents an order
#[derive(Debug, Clone)]
pub struct Order {
    pub symbol: String,
    pub quantity: f64,
    pub price: f64,
    pub side: OrderSide,
}

#[derive(Debug, Clone)]
pub enum OrderSide {
    Buy,
    Sell,
    Short,
    Cover,
}

/// Unified broker interface
#[async_trait]
pub trait BrokerClient: Send + Sync {
    async fn get_balance(&self) -> Result<f64>;
    async fn get_positions(&self) -> Result<Vec<Position>>;
    async fn buy(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order>;
    async fn sell(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order>;
    async fn short(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order>;
    async fn cover(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order>;
}

/// Depot gRPC client
pub mod depot_proto {
    tonic::include_proto!("depot");
}

pub struct DepotClient {
    client: depot_proto::depot_client::DepotClient<Channel>,
}

impl DepotClient {
    pub async fn connect(addr: &str) -> Result<Self> {
        let client = depot_proto::depot_client::DepotClient::connect(addr.to_string()).await?;
        Ok(Self { client })
    }
}

#[async_trait]
impl BrokerClient for DepotClient {
    async fn get_balance(&self) -> Result<f64> {
        let mut client = self.client.clone();
        let response = client
            .get_state(tonic::Request::new(depot_proto::Empty {}))
            .await?;
        Ok(response.into_inner().cash)
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        let mut client = self.client.clone();
        let response = client
            .get_state(tonic::Request::new(depot_proto::Empty {}))
            .await?;

        let positions = response
            .into_inner()
            .shares
            .into_iter()
            .map(|share| {
                // Depot uses positive count for long, negative for short
                let (quantity, position_type) = if share.count >= 0 {
                    (share.count as f64, PositionType::Long)
                } else {
                    ((-share.count) as f64, PositionType::Short)
                };

                Position {
                    symbol: share.symbol,
                    quantity,
                    avg_price: share.price_per_share,
                    current_price: share.price_per_share, // TODO: Get real-time price
                    position_type,
                }
            })
            .collect();

        Ok(positions)
    }

    async fn buy(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        let mut client = self.client.clone();
        let request = depot_proto::BuyRequest {
            count: quantity as i32,
            price_per_share: price,
            symbol: symbol.to_string(),
        };

        let response = client.buy_shares(tonic::Request::new(request)).await?;
        let resp = response.into_inner();

        if !resp.success {
            anyhow::bail!("Buy failed: {}", resp.message);
        }

        tracing::info!(
            "[DEPOT] Buy executed: {} shares of {} at ${}",
            quantity,
            symbol,
            price
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price,
            side: OrderSide::Buy,
        })
    }

    async fn sell(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        let mut client = self.client.clone();
        let request = depot_proto::SellRequest {
            count: quantity as i32,
            price_per_share: price,
            symbol: symbol.to_string(),
        };

        let response = client.sell_shares(tonic::Request::new(request)).await?;
        let resp = response.into_inner();

        if !resp.success {
            anyhow::bail!("Sell failed: {}", resp.message);
        }

        tracing::info!(
            "[DEPOT] Sell executed: {} shares of {} at ${}",
            quantity,
            symbol,
            price
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price,
            side: OrderSide::Sell,
        })
    }

    async fn short(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        // For Depot, short selling is implemented as selling with negative count
        let mut client = self.client.clone();
        let request = depot_proto::SellRequest {
            count: -(quantity as i32), // Negative count for short
            price_per_share: price,
            symbol: symbol.to_string(),
        };

        let response = client.sell_shares(tonic::Request::new(request)).await?;
        let resp = response.into_inner();

        if !resp.success {
            anyhow::bail!("Short failed: {}", resp.message);
        }

        info!(
            "[DEPOT] Short executed: {} shares of {} at ${}",
            quantity, symbol, price
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price,
            side: OrderSide::Short,
        })
    }

    async fn cover(&self, symbol: &str, quantity: f64, price: f64) -> Result<Order> {
        // For Depot, covering a short is buying back with negative count
        let mut client = self.client.clone();
        let request = depot_proto::BuyRequest {
            count: -(quantity as i32), // Negative count to close short
            price_per_share: price,
            symbol: symbol.to_string(),
        };

        let response = client.buy_shares(tonic::Request::new(request)).await?;
        let resp = response.into_inner();

        if !resp.success {
            anyhow::bail!("Cover failed: {}", resp.message);
        }

        info!(
            "[DEPOT] Cover executed: {} shares of {} at ${}",
            quantity, symbol, price
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price,
            side: OrderSide::Cover,
        })
    }
}

/// Alpaca paper trading client using reqwest for async HTTP calls
pub struct AlpacaClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    api_secret: String,
}

impl AlpacaClient {
    pub fn new(api_key: String, api_secret: String, paper: bool) -> Self {
        let base_url = if paper {
            "https://paper-api.alpaca.markets".to_string()
        } else {
            "https://api.alpaca.markets".to_string()
        };

        Self {
            client: reqwest::Client::new(),
            base_url,
            api_key,
            api_secret,
        }
    }
}

#[async_trait]
impl BrokerClient for AlpacaClient {
    async fn get_balance(&self) -> Result<f64> {
        let url = format!("{}/v2/account", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .send()
            .await?;

        let account: serde_json::Value = response.json().await?;
        let cash = account["cash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse cash"))?
            .parse::<f64>()?;
        Ok(cash)
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        let url = format!("{}/v2/positions", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .send()
            .await?;

        let alpaca_positions: Vec<serde_json::Value> = response.json().await?;

        let positions = alpaca_positions
            .into_iter()
            .filter_map(|pos| {
                let qty: f64 = pos["qty"].as_str()?.parse().ok()?;
                let (quantity, position_type) = if qty >= 0.0 {
                    (qty, PositionType::Long)
                } else {
                    (-qty, PositionType::Short)
                };

                Some(Position {
                    symbol: pos["symbol"].as_str()?.to_string(),
                    quantity,
                    avg_price: pos["avg_entry_price"].as_str()?.parse().ok()?,
                    current_price: pos["current_price"].as_str()?.parse().ok()?,
                    position_type,
                })
            })
            .collect();

        Ok(positions)
    }

    async fn buy(&self, symbol: &str, quantity: f64, _price: f64) -> Result<Order> {
        let url = format!("{}/v2/orders", self.base_url);
        let order_body = serde_json::json!({
            "symbol": symbol,
            "qty": quantity.to_string(),
            "side": "buy",
            "type": "market",
            "time_in_force": "day"
        });

        let _response = self
            .client
            .post(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .json(&order_body)
            .send()
            .await?;

        info!(
            "[ALPACA] Buy order submitted: {} shares of {}",
            quantity, symbol
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price: 0.0, // Market order, price unknown
            side: OrderSide::Buy,
        })
    }

    async fn sell(&self, symbol: &str, quantity: f64, _price: f64) -> Result<Order> {
        let url = format!("{}/v2/orders", self.base_url);
        let order_body = serde_json::json!({
            "symbol": symbol,
            "qty": quantity.to_string(),
            "side": "sell",
            "type": "market",
            "time_in_force": "day"
        });

        let _response = self
            .client
            .post(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .json(&order_body)
            .send()
            .await?;

        tracing::info!(
            "[ALPACA] Sell order submitted: {} shares of {}",
            quantity,
            symbol
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price: 0.0, // Market order, price unknown
            side: OrderSide::Sell,
        })
    }

    async fn short(&self, symbol: &str, quantity: f64, _price: f64) -> Result<Order> {
        // In Alpaca, shorting is just selling shares you don't own
        let url = format!("{}/v2/orders", self.base_url);
        let order_body = serde_json::json!({
            "symbol": symbol,
            "qty": quantity.to_string(),
            "side": "sell",
            "type": "market",
            "time_in_force": "day"
        });

        let _response = self
            .client
            .post(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .json(&order_body)
            .send()
            .await?;

        tracing::info!(
            "[ALPACA] Short order submitted: {} shares of {}",
            quantity,
            symbol
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price: 0.0, // Market order, price unknown
            side: OrderSide::Short,
        })
    }

    async fn cover(&self, symbol: &str, quantity: f64, _price: f64) -> Result<Order> {
        // In Alpaca, covering a short is buying back the shares
        let url = format!("{}/v2/orders", self.base_url);
        let order_body = serde_json::json!({
            "symbol": symbol,
            "qty": quantity.to_string(),
            "side": "buy",
            "type": "market",
            "time_in_force": "day"
        });

        let _response = self
            .client
            .post(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .json(&order_body)
            .send()
            .await?;

        info!(
            "[ALPACA] Cover order submitted: {} shares of {}",
            quantity, symbol
        );

        Ok(Order {
            symbol: symbol.to_string(),
            quantity,
            price: 0.0, // Market order, price unknown
            side: OrderSide::Cover,
        })
    }
}

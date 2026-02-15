use crate::data_buffer::DataBuffer;
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Include the generated proto code
pub mod indicator_proto {
    tonic::include_proto!("calculate");
}

use indicator_proto::indicator_server::{Indicator, IndicatorServer};
use indicator_proto::{
    ConfigResponse, IndicatorType, ListNumbersRequest2, ListNumbersResponse, UserRequest,
};

/// gRPC service implementation for technical indicators
pub struct IndicatorGrpcService {
    data_buffer: Arc<DataBuffer>,
}

impl IndicatorGrpcService {
    pub fn new(data_buffer: Arc<DataBuffer>) -> Self {
        Self { data_buffer }
    }
}

#[tonic::async_trait]
impl Indicator for IndicatorGrpcService {
    async fn conf_reload(
        &self,
        _request: Request<UserRequest>,
    ) -> Result<Response<ConfigResponse>, Status> {
        Ok(Response::new(ConfigResponse {
            result: "Config reload not implemented".to_string(),
        }))
    }

    async fn gen_liste(
        &self,
        request: Request<ListNumbersRequest2>,
    ) -> Result<Response<ListNumbersResponse>, Status> {
        let req = request.into_inner();

        // Get data: use provided list or fetch from buffer
        let data = if req.list.is_empty() {
            // TODO: For now, return error if no data provided
            // In future, we could add symbol field to request
            return Err(Status::invalid_argument(
                "No data provided. Please provide 'list' field with price data",
            ));
        } else {
            req.list
        };

        // Get options
        let period = req.opt.as_ref().map(|o| o.period as usize).unwrap_or(14);
        let multiplier = req.opt.as_ref().map(|o| o.multiplier).unwrap_or(2.0);

        // Calculate indicator based on type
        let result = match IndicatorType::try_from(req.id) {
            Ok(IndicatorType::SimpleMovingAverage) => calculate_sma(&data, period),
            Ok(IndicatorType::ExponentialMovingAverage) => calculate_ema(&data, period),
            Ok(IndicatorType::RelativeStrengthIndex) => calculate_rsi(&data, period),
            Ok(IndicatorType::BollingerBands) => {
                calculate_bollinger_bands(&data, period, multiplier)
            }
            Ok(IndicatorType::Maximum) => calculate_maximum(&data, period),
            Ok(IndicatorType::Minimum) => calculate_minimum(&data, period),
            Ok(IndicatorType::StandardDeviation) => calculate_std_dev(&data, period),
            Ok(IndicatorType::MeanAbsoluteDeviation) => calculate_mad(&data, period),
            Ok(IndicatorType::RateOfChange) => calculate_roc(&data, period),
            Ok(IndicatorType::MaxDrawdown) => calculate_max_drawdown(&data),
            Ok(IndicatorType::MaxDrawup) => calculate_max_drawup(&data),
            Err(_) => {
                return Err(Status::invalid_argument("Invalid indicator type"));
            }
        };

        Ok(Response::new(ListNumbersResponse { result }))
    }
}

/// Calculate Simple Moving Average
fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() < period {
        return vec![];
    }

    data.windows(period)
        .map(|window| window.iter().sum::<f64>() / period as f64)
        .collect()
}

/// Calculate Exponential Moving Average
fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
    if data.is_empty() || period == 0 {
        return vec![];
    }

    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut result = Vec::with_capacity(data.len());

    // First EMA is SMA
    let first_sma: f64 = data.iter().take(period).sum::<f64>() / period as f64;
    result.push(first_sma);

    // Calculate remaining EMAs
    for &price in &data[period..] {
        let prev_ema = *result.last().unwrap();
        let ema = (price - prev_ema) * multiplier + prev_ema;
        result.push(ema);
    }

    result
}

/// Calculate Relative Strength Index
fn calculate_rsi(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() < period + 1 {
        return vec![];
    }

    let mut gains = Vec::new();
    let mut losses = Vec::new();

    // Calculate price changes
    for i in 1..data.len() {
        let change = data[i] - data[i - 1];
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(change.abs());
        }
    }

    let mut result = Vec::new();

    // Calculate RSI for each window
    for i in period - 1..gains.len() {
        let avg_gain: f64 = gains[i - period + 1..=i].iter().sum::<f64>() / period as f64;
        let avg_loss: f64 = losses[i - period + 1..=i].iter().sum::<f64>() / period as f64;

        let rs = if avg_loss == 0.0 {
            100.0
        } else {
            avg_gain / avg_loss
        };

        let rsi = 100.0 - (100.0 / (1.0 + rs));
        result.push(rsi);
    }

    result
}

/// Calculate Bollinger Bands (returns middle, upper, lower interleaved)
fn calculate_bollinger_bands(data: &[f64], period: usize, multiplier: f64) -> Vec<f64> {
    if data.len() < period {
        return vec![];
    }

    let mut result = Vec::new();

    for window in data.windows(period) {
        let sma = window.iter().sum::<f64>() / period as f64;
        let variance = window.iter().map(|&x| (x - sma).powi(2)).sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();

        let upper = sma + (multiplier * std_dev);
        let lower = sma - (multiplier * std_dev);

        // Return as [middle, upper, lower]
        result.push(sma);
        result.push(upper);
        result.push(lower);
    }

    result
}

/// Calculate Maximum over rolling window
fn calculate_maximum(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() < period {
        return vec![];
    }

    data.windows(period)
        .map(|window| window.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        .collect()
}

/// Calculate Minimum over rolling window
fn calculate_minimum(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() < period {
        return vec![];
    }

    data.windows(period)
        .map(|window| window.iter().copied().fold(f64::INFINITY, f64::min))
        .collect()
}

/// Calculate Standard Deviation
fn calculate_std_dev(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() < period {
        return vec![];
    }

    data.windows(period)
        .map(|window| {
            let mean = window.iter().sum::<f64>() / period as f64;
            let variance = window.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / period as f64;
            variance.sqrt()
        })
        .collect()
}

/// Calculate Mean Absolute Deviation
fn calculate_mad(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() < period {
        return vec![];
    }

    data.windows(period)
        .map(|window| {
            let mean = window.iter().sum::<f64>() / period as f64;
            let mad = window.iter().map(|&x| (x - mean).abs()).sum::<f64>() / period as f64;
            mad
        })
        .collect()
}

/// Calculate Rate of Change
fn calculate_roc(data: &[f64], period: usize) -> Vec<f64> {
    if data.len() <= period {
        return vec![];
    }

    data.windows(period + 1)
        .map(|window| {
            let old_price = window[0];
            let new_price = window[period];
            ((new_price - old_price) / old_price) * 100.0
        })
        .collect()
}

/// Calculate Maximum Drawdown
fn calculate_max_drawdown(data: &[f64]) -> Vec<f64> {
    if data.is_empty() {
        return vec![];
    }

    let mut max_price = data[0];
    let mut max_dd = 0.0;

    for &price in data {
        if price > max_price {
            max_price = price;
        }
        let drawdown = ((max_price - price) / max_price) * 100.0;
        if drawdown > max_dd {
            max_dd = drawdown;
        }
    }

    vec![max_dd]
}

/// Calculate Maximum Drawup
fn calculate_max_drawup(data: &[f64]) -> Vec<f64> {
    if data.is_empty() {
        return vec![];
    }

    let mut min_price = data[0];
    let mut max_du = 0.0;

    for &price in data {
        if price < min_price {
            min_price = price;
        }
        let drawup = ((price - min_price) / min_price) * 100.0;
        if drawup > max_du {
            max_du = drawup;
        }
    }

    vec![max_du]
}

/// Start the indicator gRPC server
pub async fn start_indicator_server(
    data_buffer: Arc<DataBuffer>,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    use tonic::transport::Server;

    let service = IndicatorGrpcService::new(data_buffer);

    tracing::info!("[Indicator gRPC] Starting server on {}", addr);

    Server::builder()
        .add_service(IndicatorServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sma() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = calculate_sma(&data, 3);
        assert_eq!(result, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_ema() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = calculate_ema(&data, 3);
        assert!(!result.is_empty());
        assert!(result[0] > 0.0);
    }

    #[test]
    fn test_maximum() {
        let data = vec![1.0, 5.0, 3.0, 7.0, 2.0];
        let result = calculate_maximum(&data, 3);
        assert_eq!(result, vec![5.0, 7.0, 7.0]);
    }

    #[test]
    fn test_minimum() {
        let data = vec![5.0, 1.0, 3.0, 7.0, 2.0];
        let result = calculate_minimum(&data, 3);
        assert_eq!(result, vec![1.0, 1.0, 2.0]);
    }

    #[test]
    fn test_roc() {
        let data = vec![100.0, 105.0, 110.0, 108.0];
        let result = calculate_roc(&data, 2);
        assert!(!result.is_empty());
    }
}

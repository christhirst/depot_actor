use crate::indicator_server::indicator_proto;
use anyhow::Result;
use tonic::transport::Channel;

/// Helper client for calling the indicator service
pub struct IndicatorClient {
    client: indicator_proto::indicator_client::IndicatorClient<Channel>,
}

impl IndicatorClient {
    /// Connect to the indicator service
    pub async fn connect(addr: &str) -> Result<Self> {
        let client =
            indicator_proto::indicator_client::IndicatorClient::connect(addr.to_string()).await?;
        Ok(Self { client })
    }

    /// Calculate Simple Moving Average
    pub async fn calculate_sma(&mut self, data: Vec<f64>, period: usize) -> Result<Vec<f64>> {
        let request = tonic::Request::new(indicator_proto::ListNumbersRequest2 {
            id: indicator_proto::IndicatorType::SimpleMovingAverage as i32,
            opt: Some(indicator_proto::Opt {
                multiplier: 0.0,
                period: period as i64,
            }),
            list: data,
        });

        let response = self.client.gen_liste(request).await?;
        Ok(response.into_inner().result)
    }

    /// Calculate Exponential Moving Average
    pub async fn calculate_ema(&mut self, data: Vec<f64>, period: usize) -> Result<Vec<f64>> {
        let request = tonic::Request::new(indicator_proto::ListNumbersRequest2 {
            id: indicator_proto::IndicatorType::ExponentialMovingAverage as i32,
            opt: Some(indicator_proto::Opt {
                multiplier: 0.0,
                period: period as i64,
            }),
            list: data,
        });

        let response = self.client.gen_liste(request).await?;
        Ok(response.into_inner().result)
    }

    /// Calculate Relative Strength Index
    pub async fn calculate_rsi(&mut self, data: Vec<f64>, period: usize) -> Result<Vec<f64>> {
        let request = tonic::Request::new(indicator_proto::ListNumbersRequest2 {
            id: indicator_proto::IndicatorType::RelativeStrengthIndex as i32,
            opt: Some(indicator_proto::Opt {
                multiplier: 0.0,
                period: period as i64,
            }),
            list: data,
        });

        let response = self.client.gen_liste(request).await?;
        Ok(response.into_inner().result)
    }

    /// Calculate Bollinger Bands
    pub async fn calculate_bollinger_bands(
        &mut self,
        data: Vec<f64>,
        period: usize,
        multiplier: f64,
    ) -> Result<Vec<f64>> {
        let request = tonic::Request::new(indicator_proto::ListNumbersRequest2 {
            id: indicator_proto::IndicatorType::BollingerBands as i32,
            opt: Some(indicator_proto::Opt {
                multiplier,
                period: period as i64,
            }),
            list: data,
        });

        let response = self.client.gen_liste(request).await?;
        Ok(response.into_inner().result)
    }
}

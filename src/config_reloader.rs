use crate::grpc_server::config_proto::{
    config_service_server::ConfigService, AddSymbolRequest, AddSymbolResponse, ReloadConfigRequest,
    ReloadConfigResponse, RemoveSymbolRequest, RemoveSymbolResponse, UpdateConfigRequest,
    UpdateConfigResponse,
};
use crate::settings::Settings;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

/// Configuration Service Implementation
pub struct ConfigGrpcService {
    /// Shared settings (async-safe RwLock — no blocking inside Tonic handlers)
    settings: Arc<RwLock<Settings>>,
    /// Channel to broadcast configuration updates
    reload_tx: tokio::sync::broadcast::Sender<()>,
}

impl ConfigGrpcService {
    pub fn new(
        settings: Arc<RwLock<Settings>>,
        reload_tx: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            settings,
            reload_tx,
        }
    }
}

/// Start the config gRPC server on all interfaces at the given port.
pub async fn start_config_server(service: ConfigGrpcService, port: u16) -> anyhow::Result<()> {
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    crate::grpc_server::start_grpc_server(service, addr).await
}

#[tonic::async_trait]
impl ConfigService for ConfigGrpcService {
    /// Reload configuration from disk and update the shared state.
    async fn reload_config(
        &self,
        _request: Request<ReloadConfigRequest>,
    ) -> Result<Response<ReloadConfigResponse>, Status> {
        match Settings::load() {
            Ok(new_settings) => {
                *self.settings.write().await = new_settings;
                let _ = self.reload_tx.send(()); // Ignore error if no listeners
                Ok(Response::new(ReloadConfigResponse {
                    success: true,
                    message: "Configuration reloaded successfully".to_string(),
                }))
            }
            Err(e) => Ok(Response::new(ReloadConfigResponse {
                success: false,
                message: format!("Failed to reload configuration: {}", e),
            })),
        }
    }

    /// Replace the running configuration from a gRPC request payload.
    async fn update_config(
        &self,
        request: Request<UpdateConfigRequest>,
    ) -> Result<Response<UpdateConfigResponse>, Status> {
        let payload = request.into_inner();
        if payload.database.is_none() {
            return Err(Status::invalid_argument(
                "`database` is required in UpdateConfigRequest",
            ));
        }

        let mut settings = self.settings.write().await;

        settings.symbols = payload
            .symbols
            .into_iter()
            .map(crate::settings::SymbolConfig::from_proto)
            .collect();

        settings.database =
            crate::settings::DatabaseSettings::from_proto(payload.database.unwrap());

        let _ = self.reload_tx.send(()); // Ignore error if no listeners
        Ok(Response::new(UpdateConfigResponse {
            success: true,
            message: "Configuration updated successfully".to_string(),
        }))
    }

    /// Add a new symbol to the configuration.
    async fn add_symbol(
        &self,
        request: Request<AddSymbolRequest>,
    ) -> Result<Response<AddSymbolResponse>, Status> {
        let payload = request.into_inner();
        let new_symbol = match payload.symbol {
            Some(sym) => crate::settings::SymbolConfig::from_proto(sym),
            None => {
                return Err(Status::invalid_argument(
                    "`symbol` is required in AddSymbolRequest",
                ));
            }
        };

        let mut settings = self.settings.write().await;

        // Prevent duplicate symbols
        if settings.symbols.iter().any(|s| s.name == new_symbol.name) {
            return Ok(Response::new(AddSymbolResponse {
                success: false,
                message: format!("Symbol '{}' already exists", new_symbol.name),
            }));
        }

        settings.symbols.push(new_symbol);

        let _ = self.reload_tx.send(()); // Trigger reload

        Ok(Response::new(AddSymbolResponse {
            success: true,
            message: "Symbol added successfully. Restarting services...".to_string(),
        }))
    }

    /// Remove an existing symbol from the configuration.
    async fn remove_symbol(
        &self,
        request: Request<RemoveSymbolRequest>,
    ) -> Result<Response<RemoveSymbolResponse>, Status> {
        let payload = request.into_inner();
        let name_to_remove = payload.name;

        if name_to_remove.is_empty() {
            return Err(Status::invalid_argument(
                "`name` is required in RemoveSymbolRequest",
            ));
        }

        let mut settings = self.settings.write().await;

        let initial_len = settings.symbols.len();
        settings.symbols.retain(|s| s.name != name_to_remove);

        if settings.symbols.len() == initial_len {
            return Ok(Response::new(RemoveSymbolResponse {
                success: false,
                message: format!("Symbol '{}' not found in configuration", name_to_remove),
            }));
        }

        let _ = self.reload_tx.send(()); // Trigger reload

        Ok(Response::new(RemoveSymbolResponse {
            success: true,
            message: format!(
                "Symbol '{}' removed successfully. Restarting services...",
                name_to_remove
            ),
        }))
    }
}

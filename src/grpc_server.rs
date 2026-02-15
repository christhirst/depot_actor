use crate::config_reloader::ConfigReloader;
use crate::settings::Settings;
use tonic::{transport::Server, Request, Response, Status};

// Include the generated proto code
pub mod config_proto {
    tonic::include_proto!("config");
}

use config_proto::config_service_server::{ConfigService, ConfigServiceServer};
use config_proto::{
    ReloadConfigRequest, ReloadConfigResponse, UpdateConfigRequest, UpdateConfigResponse,
};

/// gRPC service implementation for config reload
pub struct ConfigGrpcService {
    config_reloader: ConfigReloader<Settings, config::ConfigError>,
}

impl ConfigGrpcService {
    pub fn new(config_reloader: ConfigReloader<Settings, config::ConfigError>) -> Self {
        Self { config_reloader }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigGrpcService {
    async fn reload_config(
        &self,
        _request: Request<ReloadConfigRequest>,
    ) -> Result<Response<ReloadConfigResponse>, Status> {
        println!("[gRPC] Received config reload request");

        match self.config_reloader.reload() {
            Ok(()) => {
                println!("[gRPC] Config reloaded successfully");
                Ok(Response::new(ReloadConfigResponse {
                    success: true,
                    message: "Configuration reloaded successfully".to_string(),
                }))
            }
            Err(e) => {
                eprintln!("[gRPC] Failed to reload config: {:?}", e);
                Ok(Response::new(ReloadConfigResponse {
                    success: false,
                    message: format!("Failed to reload configuration: {}", e),
                }))
            }
        }
    }

    async fn update_config(
        &self,
        request: Request<UpdateConfigRequest>,
    ) -> Result<Response<UpdateConfigResponse>, Status> {
        println!("[gRPC] Received config update request");

        let proto_config = request.into_inner();
        let new_config = Settings::from_proto(proto_config);

        match self.config_reloader.update(new_config) {
            Ok(()) => {
                println!("[gRPC] Config updated successfully");
                Ok(Response::new(UpdateConfigResponse {
                    success: true,
                    message: "Configuration updated successfully".to_string(),
                }))
            }
            Err(e) => {
                eprintln!("[gRPC] Failed to update config: {:?}", e);
                Ok(Response::new(UpdateConfigResponse {
                    success: false,
                    message: format!("Failed to update configuration: {}", e),
                }))
            }
        }
    }
}

/// Start the gRPC server
pub async fn start_grpc_server(
    config_reloader: ConfigReloader<Settings, config::ConfigError>,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let service = ConfigGrpcService::new(config_reloader);

    println!("[gRPC] Starting server on {}", addr);

    Server::builder()
        .add_service(ConfigServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

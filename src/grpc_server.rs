use crate::trader::config_reloader::ConfigGrpcService;
use tonic::transport::Server;

// Re-export generated proto code so other modules can use it
pub mod config_proto {
    tonic::include_proto!("config");
}

use config_proto::config_service_server::ConfigServiceServer;

/// Start the gRPC server
pub async fn start_grpc_server(
    service: ConfigGrpcService,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    tracing::info!("[gRPC] Starting server on {}", addr);

    Server::builder()
        .add_service(ConfigServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::{net::SocketAddr, sync::Arc};
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing_subscriber::prelude::*;

mod server;
mod types;
mod handlers;
mod utils;

use server::Server;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mpp_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let server = Arc::new(Server::new());

    let app = Router::new()
        .route("/ws", get(ws_handler)) // Idk how to get this to stay on "/" without getting "Connection header did not include 'upgrade'"
        .fallback_service(ServeDir::new("client").append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(server.clone());

    let port = std::env::var("WS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Server running on port: {}", port);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>()
    )
    .await
    .expect("Server error");
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(server): axum::extract::State<Arc<Server>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, server, addr))
}

async fn handle_socket(socket: WebSocket, server: Arc<Server>, addr: SocketAddr) {
    let ip = addr.ip().to_string();
    
    if let Err(e) = server.handle_connection(socket, ip).await {
        tracing::error!("Error handling connection: {}", e);
    }
}
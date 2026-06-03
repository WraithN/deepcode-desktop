use super::connection::handle_connection;
use super::router::GatewayRouter;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

pub struct WebSocketServer {
    pub addr: SocketAddr,
    router: Arc<GatewayRouter>,
}

impl WebSocketServer {
    pub fn new(router: Arc<GatewayRouter>) -> Self {
        Self {
            addr: "127.0.0.1:0".parse().unwrap(),
            router,
        }
    }

    pub async fn start(
        &mut self,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        self.addr = listener.local_addr()?;

        log::info!("WebSocket server listening on {}", self.addr);

        let router = self.router.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Ok((stream, _)) = listener.accept() => {
                        let router = router.clone();
                        let conn_id = format!("conn-{}", uuid::Uuid::new_v4());

                        tokio::spawn(async move {
                            match tokio_tungstenite::accept_async(stream).await {
                                Ok(ws_stream) => {
                                    handle_connection(conn_id, ws_stream, router).await;
                                }
                                Err(e) => {
                                    log::error!("WebSocket handshake failed: {}", e);
                                }
                            }
                        });
                    }
                    _ = shutdown.recv() => {
                        log::info!("WebSocket server shutting down");
                        break;
                    }
                }
            }
        });

        Ok(self.addr)
    }
}

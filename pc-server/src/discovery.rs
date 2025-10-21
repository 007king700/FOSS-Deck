use anyhow::Result;
use tokio::sync::oneshot;
use whoami::fallible;

pub async fn run_discovery_server(ws_port: u16, mut shutdown_rx: oneshot::Receiver<()>) -> Result<()> {
    use log::{error, info};
    use serde_json::json;
    use tokio::{net::UdpSocket, select};

    const DISCOVERY_PORT: u16 = 45321;
    const QUERY: &str = "FOSSDECK_DISCOVERY_V1?";

    let sock = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await?;
    info!("Discovery listening on UDP {DISCOVERY_PORT}");

    let mut buf = [0u8; 1024];

    loop {
        select! {
            _ = &mut shutdown_rx => break,
            res = sock.recv_from(&mut buf) => {
                match res {
                    Ok((n, peer)) => {
                        let msg = std::str::from_utf8(&buf[..n]).unwrap_or_default();
                        if msg == QUERY {
                            let name = fallible::hostname().unwrap_or_else(|_| "unknown".to_string());
                            let reply = json!({
                                "name": name,
                                "proto": "ws",
                                "port": ws_port,
                                "path": "/ws",
                                "version": "0.1.0"
                            }).to_string();
                            if let Err(e) = sock.send_to(reply.as_bytes(), peer).await {
                                error!("discovery send_to error: {e}");
                            }
                        }
                    }
                    Err(e) => { error!("discovery recv_from error: {e}"); }
                }
            }
        }
    }
    Ok(())
}

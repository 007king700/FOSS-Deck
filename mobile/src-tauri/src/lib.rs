use get_if_addrs::get_if_addrs;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscoveredHost {
    pub ip: String,
    pub port: u16,
    pub name: Option<String>,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[tauri::command]
async fn discover_hosts(timeout_ms: Option<u64>) -> Result<Vec<DiscoveredHost>, String> {
    use std::time::Instant;
    use tokio::net::UdpSocket;
    use tokio::time::{timeout, Duration};

    const DISCOVERY_PORT: u16 = 45321;
    const QUERY: &[u8] = b"FOSSDECK_DISCOVERY_V1?";

    let sock = UdpSocket::bind(("0.0.0.0", 0)).await.map_err(|e| e.to_string())?;
    sock.set_broadcast(true).map_err(|e| e.to_string())?;

    // Build broadcast targets (255.255.255.255 + each interface directed broadcast)
    let mut targets = vec![format!("255.255.255.255:{DISCOVERY_PORT}")];
    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() { continue; }
            if let std::net::IpAddr::V4(_ip) = iface.ip() {
                if let get_if_addrs::IfAddr::V4(v4_addr) = iface.addr {
                    let ip = v4_addr.ip;
                    let mask = v4_addr.netmask;
                    let ip_u = u32::from(ip);
                    let mask_u = u32::from(mask);
                    let bcast = std::net::Ipv4Addr::from(ip_u | !mask_u);
                    targets.push(format!("{bcast}:{DISCOVERY_PORT}"));
                }
            }
        }
    }
    targets.sort();
    targets.dedup();

    for t in &targets {
        let _ = sock.send_to(QUERY, t).await;
    }

    let deadline = Instant::now() + Duration::from_millis(timeout_ms.unwrap_or(1200));
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = vec![];
    let mut buf = [0u8; 2048];

    loop {
        let now = Instant::now();
        if now >= deadline { break; }
        match timeout(deadline.saturating_duration_since(now), sock.recv_from(&mut buf)).await {
            Ok(Ok((n, addr))) => {
                let ip = addr.ip().to_string();
                if !seen.insert(ip.clone()) {
                    continue;
                }
                let text = std::str::from_utf8(&buf[..n]).unwrap_or("");
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                    let port = v.get("port").and_then(|p| p.as_u64()).unwrap_or(3030) as u16;
                    let name = v.get("name").and_then(|s| s.as_str()).map(|s| s.to_string());
                    let path = v.get("path").and_then(|s| s.as_str()).map(|s| s.to_string());
                    let version = v.get("version").and_then(|s| s.as_str()).map(|s| s.to_string());
                    out.push(DiscoveredHost { ip, port, name, path, version });
                } else {
                    out.push(DiscoveredHost { ip, port: 3030, name: None, path: Some("/ws".into()), version: None });
                }
            }
            _ => break, // timeout
        }
    }

    Ok(out)
}

#[tauri::mobile_entry_point]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![discover_hosts])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

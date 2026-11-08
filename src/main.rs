#[allow(dead_code)]
mod protocol;
mod relay;
mod security;
mod socks4;
mod socks5;
mod ssh;
mod tcp_fallback;
mod tls;
mod websocket;

use protocol::Protocol;
use std::env;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::time::{timeout, Duration};

const BYPASS_DIR: &str = "/opt/mpro";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let port = get_port();
    let status = get_status();
    let use_udp = has_arg("-u");
    let use_quic = has_arg("-q");
    let bypass_enabled = bypass_enabled_for_port(port);
    let multiprotocol_enabled = multiprotocol_enabled_for_port(port);
    let ssh_only = has_arg("-ssh");
    let tls_enabled = has_arg("-t") || !has_arg("--no-tls");
    let quic_port = get_quic_port();

    println!("[Azpro] Proxy TCP multiprotocolo v2.6.0");
    println!("[TCP] Porta: {}", port);
    if multiprotocol_enabled {
        println!(
            "[MULTIPROTOCOLO] Detecção automática habilitada; status: {}",
            status
        );
    }
    if bypass_enabled {
        println!("[BYPASS] SSH_PROXY com payload HTTP ativo nesta porta");
    }

    if use_udp {
        println!("[UDP] Habilitado na porta: {}", port);
        let status_udp = status.clone();
        tokio::spawn(async move {
            if let Err(e) = start_udp(port, &status_udp).await {
                println!("[UDP] Erro: {}", e);
            }
        });
    }

    if use_quic {
        println!("[QUIC] Habilitado na porta: {}", quic_port);
        // QUIC permanece desativado: o binário não possui um listener QUIC real.
    }

    let listener = TcpListener::bind(format!("[::]:{}", port)).await?;
    println!("Aguardando conexoes TCP...");
    start_http(
        listener,
        status,
        ssh_only,
        bypass_enabled,
        multiprotocol_enabled,
        tls_enabled,
    )
    .await;
    Ok(())
}

async fn start_http(
    listener: TcpListener,
    status: String,
    ssh_only: bool,
    bypass_enabled: bool,
    multiprotocol_enabled: bool,
    tls_enabled: bool,
) {
    let status_arc = Arc::new(status);
    loop {
        match listener.accept().await {
            Ok((client_stream, addr)) => {
                let _ = client_stream.set_nodelay(true);
                let status = status_arc.clone();
                tokio::spawn(async move {
                    let result = handle_client_protocol(
                        client_stream,
                        &status,
                        ssh_only,
                        bypass_enabled,
                        multiprotocol_enabled,
                        tls_enabled,
                    )
                    .await;
                    if let Err(e) = result {
                        println!("Erro ao processar cliente {}: {}", addr, e);
                    }
                });
            }
            Err(e) => println!("Erro ao aceitar conexao: {}", e),
        }
    }
}

async fn handle_client_protocol(
    client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
    bypass_enabled: bool,
    multiprotocol_enabled: bool,
    tls_enabled: bool,
) -> Result<(), Error> {
    if multiprotocol_enabled {
        return handle_multiprotocol_client(client_stream, status, ssh_only, tls_enabled).await;
    }

    if tls_enabled && peek_tls_client_hello(&client_stream).await? {
        println!("[TLS] ClientHello detectado; iniciando passthrough para SSH/VPN local");
        return tls::handle_tls(client_stream)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, e.to_string()));
    }
    if bypass_enabled {
        return handle_bypass_http_client(client_stream, status).await;
    }
    handle_client(client_stream, status, ssh_only).await
}

async fn handle_multiprotocol_client(
    client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
    tls_enabled: bool,
) -> Result<(), Error> {
    let detected = protocol::detect_from_stream(&client_stream).await?;
    println!("[MULTIPROTOCOLO] Detectado: {:?}", detected);

    match detected {
        Protocol::Socks4 => socks4::handle_socks4(client_stream)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("SOCKS4: {e}"))),
        Protocol::Socks5 => socks5::handle_socks5(client_stream)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("SOCKS5: {e}"))),
        Protocol::Tls if tls_enabled => tls::handle_tls(client_stream)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("TLS passthrough: {e}"))),
        Protocol::Tls => Err(Error::new(
            ErrorKind::InvalidData,
            "TLS desabilitado nesta porta",
        )),
        Protocol::WebSocket => websocket::handle_websocket(client_stream, status)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("WebSocket: {e}"))),
        Protocol::Security => security::handle_security(client_stream, status, ssh_only)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("SECURITY: {e}"))),
        Protocol::Http => handle_multiprotocol_http_client(client_stream, status, ssh_only).await,
        Protocol::SshDirect => ssh::handle_ssh_tunnel(client_stream, "127.0.0.1:22")
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("SSH direto: {e}"))),
        Protocol::Unknown => tcp_fallback::handle_tcp(client_stream)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("TCP fallback local: {e}"))),
    }
}

fn bypass_enabled_for_port(port: u16) -> bool {
    bypass_enabled_in(port, Path::new(BYPASS_DIR))
}

fn bypass_enabled_in(port: u16, directory: &Path) -> bool {
    directory
        .join(format!("bypass-http-ssh-{port}.enabled"))
        .is_file()
}

fn multiprotocol_enabled_for_port(port: u16) -> bool {
    multiprotocol_enabled_in(port, Path::new(BYPASS_DIR))
}

fn multiprotocol_enabled_in(port: u16, directory: &Path) -> bool {
    directory
        .join(format!("multiprotocol-{port}.enabled"))
        .is_file()
}

/// BYPASS SSH_PROXY usa o mesmo handshake HTTP do AWProxy e força o backend SSH.
async fn handle_bypass_http_client(client_stream: TcpStream, status: &str) -> Result<(), Error> {
    println!("[BYPASS][HTTP] Encaminhando SSH para 127.0.0.1:22");
    handle_legacy_http_tunnel(client_stream, status, true, true).await
}

/// Handler original da opção 1, mantido sem as mudanças de multiprotocolo.
async fn handle_client(
    client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    handle_legacy_http_tunnel(client_stream, status, ssh_only, false).await
}

async fn handle_multiprotocol_http_client(
    client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    handle_multiprotocol_http_tunnel(client_stream, status, ssh_only).await
}

async fn handle_legacy_http_tunnel(
    mut client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
    force_ssh: bool,
) -> Result<(), Error> {
    client_stream.set_nodelay(true)?;
    client_stream
        .write_all(&http_status_response(101, status))
        .await?;
    client_stream.flush().await?;

    let mut initial_payload = [0u8; 1024];
    let _ = client_stream.read(&mut initial_payload).await?;
    client_stream
        .write_all(&http_legacy_followup(status))
        .await?;
    client_stream.flush().await?;

    let addr_proxy = if force_ssh || ssh_only {
        "127.0.0.1:22"
    } else {
        match timeout(Duration::from_secs(1), peek_stream(&client_stream)).await {
            Ok(Ok(data)) if data.contains("SSH") || data.is_empty() => "127.0.0.1:22",
            Ok(_) => "127.0.0.1:1194",
            Err(_) => "127.0.0.1:22",
        }
    };
    println!("[HTTP] Backend selecionado: {}", addr_proxy);
    relay_to_backend_legacy(client_stream, addr_proxy).await?;
    Ok(())
}

async fn handle_multiprotocol_http_tunnel(
    mut client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    client_stream.set_nodelay(true)?;
    client_stream
        .write_all(&http_status_response(101, status))
        .await?;
    client_stream.flush().await?;

    let mut initial_payload = [0u8; 1024];
    let initial_len = client_stream.read(&mut initial_payload).await?;

    let initial_text = String::from_utf8_lossy(&initial_payload[..initial_len]);
    let addr_proxy = if ssh_only {
        "127.0.0.1:22"
    } else {
        if initial_text.contains("SSH") || initial_len == 0 {
            "127.0.0.1:22"
        } else {
            "127.0.0.1:1194"
        }
    };
    println!("[HTTP] Backend selecionado: {}", addr_proxy);
    client_stream
        .write_all(&http_handshake_followup(status))
        .await?;
    client_stream.flush().await?;
    relay_to_backend(client_stream, addr_proxy, &initial_payload[..initial_len]).await
}

fn http_status_response(code: u16, status: &str) -> Vec<u8> {
    format!("HTTP/1.1 {code} {status}\r\n\r\n").into_bytes()
}

fn http_handshake_followup(status: &str) -> Vec<u8> {
    let mut response = http_status_response(101, status);
    response.extend_from_slice(&http_status_response(200, status));
    response
}

fn http_legacy_followup(status: &str) -> Vec<u8> {
    http_status_response(200, status)
}

async fn relay_to_backend_legacy(mut client_stream: TcpStream, addr: &str) -> Result<(), Error> {
    let mut server_stream = timeout(Duration::from_secs(3), TcpStream::connect(addr))
        .await
        .map_err(|_| Error::new(ErrorKind::TimedOut, "timeout conectando backend"))??;
    server_stream.set_nodelay(true)?;
    let _ = relay::copy_bidirectional(&mut client_stream, &mut server_stream).await;
    Ok(())
}

async fn relay_to_backend(
    mut client_stream: TcpStream,
    addr: &str,
    initial_payload: &[u8],
) -> Result<(), Error> {
    let mut server_stream = timeout(Duration::from_secs(3), TcpStream::connect(addr))
        .await
        .map_err(|_| Error::new(ErrorKind::TimedOut, "timeout conectando backend"))??;
    server_stream.set_nodelay(true)?;
    if !initial_payload.is_empty() {
        server_stream.write_all(initial_payload).await?;
    }
    let _ = relay::copy_bidirectional(&mut client_stream, &mut server_stream).await;
    Ok(())
}

async fn start_udp(port: u16, _status: &str) -> Result<(), Error> {
    let socket = UdpSocket::bind(format!("[::]:{}", port)).await?;
    let mut buf = [0u8; 2048];
    loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        println!("[UDP] {} bytes de {}", len, addr);
    }
}

async fn peek_tls_client_hello(stream: &TcpStream) -> Result<bool, Error> {
    let mut first_byte = [0u8; 1];
    match timeout(Duration::from_millis(50), stream.peek(&mut first_byte)).await {
        Ok(Ok(bytes)) => Ok(bytes > 0 && first_byte[0] == 0x16),
        Ok(Err(error)) => Err(error),
        Err(_) => Ok(false),
    }
}

async fn peek_stream(stream: &TcpStream) -> Result<String, Error> {
    let mut peek_buffer = vec![0; 8192];
    let bytes_peeked = stream.peek(&mut peek_buffer).await?;
    Ok(String::from_utf8_lossy(&peek_buffer[..bytes_peeked]).to_string())
}

fn get_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    for i in 1..args.len() {
        if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            return args[i + 1].parse().unwrap_or(80);
        }
    }
    80
}

fn get_quic_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    for i in 1..args.len() {
        if args[i] == "--quic-port" && i + 1 < args.len() {
            return args[i + 1].parse().unwrap_or(8001);
        }
    }
    8001
}

fn get_status() -> String {
    let args: Vec<String> = env::args().collect();
    for i in 1..args.len() {
        if (args[i] == "--status" || args[i] == "-s") && i + 1 < args.len() {
            return args[i + 1].clone();
        }
    }
    "@Azpro".to_string()
}

fn has_arg(arg: &str) -> bool {
    env::args().any(|a| a == arg)
}

#[cfg(test)]
mod tests {
    use super::{
        bypass_enabled_in, http_handshake_followup, http_legacy_followup, http_status_response,
        multiprotocol_enabled_in,
    };
    use std::path::Path;

    #[test]
    fn per_port_markers_do_not_enable_other_ports_or_each_other() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("azpro-markers-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("multiprotocol-80.enabled"), b"").unwrap();

        assert!(multiprotocol_enabled_in(80, &directory));
        assert!(!multiprotocol_enabled_in(443, &directory));
        assert!(!bypass_enabled_in(80, &directory));
        std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    }

    #[test]
    fn configured_status_is_used_for_each_http_handshake_response() {
        assert_eq!(
            http_status_response(101, "@Azpro"),
            b"HTTP/1.1 101 @Azpro\r\n\r\n"
        );
        assert_eq!(
            http_status_response(200, "@Azpro"),
            b"HTTP/1.1 200 @Azpro\r\n\r\n"
        );
    }

    #[test]
    fn http_handshake_followup_emits_second_101_then_200_with_same_status() {
        assert_eq!(
            http_status_response(101, "Aznet"),
            b"HTTP/1.1 101 Aznet\r\n\r\n"
        );
        assert_eq!(
            http_handshake_followup("Aznet"),
            b"HTTP/1.1 101 Aznet\r\n\r\nHTTP/1.1 200 Aznet\r\n\r\n"
        );
    }

    #[test]
    fn option_one_keeps_legacy_single_101_then_200_handshake() {
        assert_eq!(
            http_status_response(101, "Aznet"),
            b"HTTP/1.1 101 Aznet\r\n\r\n"
        );
        assert_eq!(http_legacy_followup("Aznet"), b"HTTP/1.1 200 Aznet\r\n\r\n");
    }
}

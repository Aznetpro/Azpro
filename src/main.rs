mod protocol;
mod relay;
mod security;
mod socks5;
mod ssh;
mod tcp_fallback;
mod tls;
mod websocket;

use std::env;
use std::io::Error;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::time::{timeout, Duration};

const MULTIPROTOCOL_DIR: &str = "/opt/mpro";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let port = get_port();
    let status = get_status();
    let use_udp = has_arg("-u");
    let use_quic = has_arg("-q");
    let multiprotocol_enabled = multiprotocol_enabled_for_port(port);
    let ssh_only = has_arg("-ssh");
    let quic_port = get_quic_port();

    println!("[WZPRO] Multi-protocolo v2.5.0");
    println!("[TCP] Porta: {}", port);
    if multiprotocol_enabled {
        println!("[TCP] Multiprotocolo + multi-status ativo nesta porta");
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
        // Implementação simplificada de QUIC placeholder.
    }

    let listener = TcpListener::bind(format!("[::]:{}", port)).await?;
    println!("Aguardando conexoes TCP...");
    start_http(listener, status, ssh_only, multiprotocol_enabled).await;
    Ok(())
}

async fn start_http(
    listener: TcpListener,
    status: String,
    ssh_only: bool,
    multiprotocol_enabled: bool,
) {
    let status_arc = Arc::new(status);
    loop {
        match listener.accept().await {
            Ok((client_stream, addr)) => {
                let _ = client_stream.set_nodelay(true);
                let status = status_arc.clone();
                tokio::spawn(async move {
                    let result = if multiprotocol_enabled {
                        handle_multiprotocol_client(client_stream, &status, ssh_only).await
                    } else {
                        handle_client(client_stream, &status, ssh_only).await
                    };
                    if let Err(e) = result {
                        println!("Erro ao processar cliente {}: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                println!("Erro ao aceitar conexao: {}", e);
            }
        }
    }
}

fn multiprotocol_enabled_for_port(port: u16) -> bool {
    multiprotocol_enabled_in(port, Path::new(MULTIPROTOCOL_DIR))
}

fn multiprotocol_enabled_in(port: u16, directory: &Path) -> bool {
    directory.join("multiprotocol.enabled").is_file()
        || directory
            .join(format!("multiprotocol-{port}.enabled"))
            .is_file()
}

/// Despacha cada conexão da opção 01 sem consumir o prefixo usado para detectá-la.
/// Cada protocolo recebe exclusivamente seu handshake próprio.
async fn handle_multiprotocol_client(
    client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    let detected = protocol::detect_from_stream(&client_stream).await?;
    println!("[MULTI] Protocolo detectado: {:?}", detected);

    match detected {
        protocol::Protocol::Socks5 => socks5::handle_socks5(client_stream)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Tls => tls::handle_tls(client_stream)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::WebSocket => websocket::handle_websocket(client_stream, status)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Security => security::handle_security(client_stream, status)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Http => {
            handle_multiprotocol_http_client(client_stream, status, ssh_only).await
        }
        protocol::Protocol::SshDirect => ssh::handle_ssh_tunnel(client_stream, "127.0.0.1:22")
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Unknown => tcp_fallback::handle_tcp(client_stream)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
    }
}

/// Handshake HTTP customizado 101/101/200 usado somente no dispatcher multiprotocolo.
async fn handle_multiprotocol_http_client(
    mut client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    client_stream.set_nodelay(true)?;
    client_stream
        .write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
        .await?;
    client_stream.flush().await?;

    let mut buffer = [0u8; 1024];
    let bytes_read = timeout(Duration::from_secs(5), client_stream.read(&mut buffer))
        .await
        .map_err(|_| Error::new(std::io::ErrorKind::TimedOut, "timeout lendo payload HTTP"))??;
    if bytes_read == 0 {
        return Ok(());
    }

    let followup = multiprotocol_http_followup(status);
    client_stream.write_all(&followup).await?;
    client_stream.flush().await?;

    let addr_proxy = if ssh_only {
        "127.0.0.1:22"
    } else {
        match timeout(Duration::from_millis(200), peek_stream(&client_stream)).await {
            Ok(Ok(data)) if data.contains("SSH") || data.is_empty() => "127.0.0.1:22",
            Ok(_) => "127.0.0.1:1194",
            Err(_) => "127.0.0.1:22",
        }
    };
    println!("[MULTI][HTTP] Backend selecionado: {}", addr_proxy);
    relay_to_backend(client_stream, addr_proxy).await
}

fn multiprotocol_http_followup(status: &str) -> Vec<u8> {
    format!("HTTP/1.1 101 {status}\r\n\r\nHTTP/1.1 200 {status}\r\n\r\n").into_bytes()
}

/// Caminho HTTP legado, preservado para quando o multiprotocolo global está desligado.
async fn handle_client(
    mut client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    client_stream.set_nodelay(true)?;
    client_stream
        .write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
        .await?;

    let mut buffer = vec![0; 1024];
    let _ = client_stream.read(&mut buffer).await?;

    client_stream
        .write_all(format!("HTTP/1.1 200 {}\r\n\r\n", status).as_bytes())
        .await?;

    let addr_proxy = if ssh_only {
        "127.0.0.1:22"
    } else {
        match timeout(Duration::from_millis(200), peek_stream(&client_stream)).await {
            Ok(Ok(data)) if data.contains("SSH") || data.is_empty() => "127.0.0.1:22",
            Ok(_) => "127.0.0.1:1194",
            Err(_) => "127.0.0.1:22",
        }
    };
    println!("[HTTP] Backend selecionado: {}", addr_proxy);

    relay_to_backend(client_stream, addr_proxy).await?;
    Ok(())
}

async fn relay_to_backend(mut client_stream: TcpStream, addr: &str) -> Result<(), Error> {
    let mut server_stream = timeout(Duration::from_secs(3), TcpStream::connect(addr))
        .await
        .map_err(|_| Error::new(std::io::ErrorKind::TimedOut, "timeout conectando backend"))??;
    server_stream.set_nodelay(true)?;
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

async fn peek_stream(stream: &TcpStream) -> Result<String, Error> {
    let mut peek_buffer = vec![0; 8192];
    let bytes_peeked = stream.peek(&mut peek_buffer).await?;
    let data = &peek_buffer[..bytes_peeked];
    Ok(String::from_utf8_lossy(data).to_string())
}

fn get_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    for i in 1..args.len() {
        if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            return args[i + 1].parse().unwrap_or(8080);
        }
    }
    8080
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
    "@Mpro".to_string()
}

fn has_arg(arg: &str) -> bool {
    env::args().any(|a| a == arg)
}

#[cfg(test)]
mod multiprotocol_tests {
    use super::{multiprotocol_enabled_in, multiprotocol_http_followup};
    use std::path::Path;

    #[test]
    fn per_port_flag_only_enables_the_selected_port_and_global_flag_still_works() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("wzpro-multiprotocol-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("multiprotocol-80.enabled"), b"").unwrap();

        assert!(multiprotocol_enabled_in(80, &directory));
        assert!(!multiprotocol_enabled_in(443, &directory));

        std::fs::write(directory.join("multiprotocol.enabled"), b"").unwrap();
        assert!(multiprotocol_enabled_in(443, &directory));
        std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    }

    #[test]
    fn multiprotocol_http_sends_second_101_then_200_with_configured_status() {
        assert_eq!(
            multiprotocol_http_followup("Wzpro"),
            b"HTTP/1.1 101 Wzpro\r\n\r\nHTTP/1.1 200 Wzpro\r\n\r\n"
        );
    }
}

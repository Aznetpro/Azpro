mod protocol;
mod relay;
mod security;
mod socks4;
mod socks5;
mod ssh;
mod tls;
mod websocket;

use std::env;
use std::io::Error;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::time::{timeout, Duration};

const BYPASS_DIR: &str = "/opt/mpro";
const MAX_HTTP_PREFACE: usize = 16 * 1024;
const HTTP_PREFACE_IDLE: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> Result<(), Error> {
    let port = get_port();
    let status = get_status();
    let use_udp = has_arg("-u");
    let use_quic = has_arg("-q");
    let bypass_enabled = bypass_enabled_for_port(port);
    let ssh_only = has_arg("-ssh");
    let tls_enabled = has_arg("-t") || !has_arg("--no-tls");
    let quic_port = get_quic_port();

    println!("[Azpro] BYPASS SSH_PROXY/HTTP v2.5.0");
    println!("[TCP] Porta: {}", port);
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
        // Implementação simplificada de QUIC placeholder.
    }

    let listener = TcpListener::bind(format!("[::]:{}", port)).await?;
    println!("Aguardando conexoes TCP...");
    start_http(listener, status, ssh_only, bypass_enabled, tls_enabled).await;
    Ok(())
}

async fn start_http(
    listener: TcpListener,
    status: String,
    ssh_only: bool,
    bypass_enabled: bool,
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
                        tls_enabled,
                    )
                    .await;
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

async fn handle_client_protocol(
    client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
    bypass_enabled: bool,
    tls_enabled: bool,
) -> Result<(), Error> {
    let detected = protocol::detect_from_stream(&client_stream).await?;

    if bypass_enabled
        && matches!(
            detected,
            protocol::Protocol::Http | protocol::Protocol::SshDirect
        )
    {
        return handle_bypass_client(client_stream, status, detected).await;
    }

    match detected {
        protocol::Protocol::Socks4 => socks4::handle_socks4(client_stream)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Socks5 => socks5::handle_socks5(client_stream)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Security => security::handle_security(client_stream, status, ssh_only)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::WebSocket => websocket::handle_websocket(client_stream, status)
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        protocol::Protocol::Http => handle_client(client_stream, status, ssh_only).await,
        protocol::Protocol::SshDirect | protocol::Protocol::Unknown => {
            println!("[SSH_DIRECT] Encaminhando conexao para 127.0.0.1:22");
            ssh::handle_ssh_tunnel(client_stream, "127.0.0.1:22")
                .await
                .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))
        }
        protocol::Protocol::Tls => {
            if !tls_enabled {
                println!("[TLS] ClientHello recusado: HTTPS/TLS desativado nesta porta");
                return Err(Error::new(
                    std::io::ErrorKind::Unsupported,
                    "HTTPS/TLS desativado para esta porta",
                ));
            }
            println!("[TLS] ClientHello detectado; iniciando passthrough para SSH/VPN local");
            tls::handle_tls(client_stream)
                .await
                .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))
        }
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

/// BYPASS dedicado a SSH_PROXY com payload HTTP. Não envia status a SSH_DIRECT.
async fn handle_bypass_client(
    client_stream: TcpStream,
    status: &str,
    detected: protocol::Protocol,
) -> Result<(), Error> {
    println!("[BYPASS] Protocolo detectado: {:?}", detected);

    match detected {
        protocol::Protocol::Http => handle_bypass_http_client(client_stream, status).await,
        protocol::Protocol::SshDirect => ssh::handle_ssh_tunnel(client_stream, "127.0.0.1:22")
            .await
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string())),
        other => Err(Error::new(
            std::io::ErrorKind::InvalidData,
            format!("BYPASS SSH_PROXY espera payload HTTP; recebido {:?}", other),
        )),
    }
}

/// Status customizados para SSH_PROXY com payload HTTP; sempre termina no SSH local.
async fn handle_bypass_http_client(
    mut client_stream: TcpStream,
    status: &str,
) -> Result<(), Error> {
    client_stream.set_nodelay(true)?;
    let _preface = read_http_preface(&mut client_stream).await?;

    let followup = bypass_http_followup(status);
    client_stream.write_all(&followup).await?;
    client_stream.flush().await?;
    println!("[BYPASS][HTTP] Encaminhando SSH para 127.0.0.1:22");
    relay_to_backend(client_stream, "127.0.0.1:22").await
}

/// Lê o preâmbulo em vários segmentos sem esperar Content-Length; envia 100 Continue
/// assim que um cabeçalho Expect correspondente for reconhecido.
async fn read_http_preface(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
    let mut preface = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let first_read = timeout(Duration::from_secs(5), stream.read(&mut chunk))
        .await
        .map_err(|_| Error::new(std::io::ErrorKind::TimedOut, "timeout lendo payload HTTP"))??;
    if first_read == 0 {
        return Err(Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "cliente fechou antes do payload HTTP",
        ));
    }
    preface.extend_from_slice(&chunk[..first_read.min(MAX_HTTP_PREFACE)]);

    let mut continue_sent = false;
    if protocol::has_expect_continue(&preface) {
        stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await?;
        stream.flush().await?;
        continue_sent = true;
    }

    while preface.len() < MAX_HTTP_PREFACE {
        let read = timeout(HTTP_PREFACE_IDLE, stream.read(&mut chunk)).await;
        match read {
            Ok(Ok(n)) if n > 0 => {
                let available = MAX_HTTP_PREFACE - preface.len();
                preface.extend_from_slice(&chunk[..n.min(available)]);
                if !continue_sent && protocol::has_expect_continue(&preface) {
                    stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await?;
                    stream.flush().await?;
                    continue_sent = true;
                }
            }
            Ok(Ok(_)) | Err(_) => break,
            Ok(Err(error)) => return Err(error),
        }
    }

    Ok(preface)
}

fn bypass_http_followup(status: &str) -> Vec<u8> {
    format!("HTTP/1.1 101 {status}\r\n\r\nHTTP/1.1 200 {status}\r\n\r\n").into_bytes()
}

/// Caminho HTTP legado, preservado quando o BYPASS está desligado.
async fn handle_client(
    mut client_stream: TcpStream,
    status: &str,
    ssh_only: bool,
) -> Result<(), Error> {
    client_stream.set_nodelay(true)?;
    let _preface = read_http_preface(&mut client_stream).await?;
    client_stream
        .write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
        .await?;
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
mod bypass_tests {
    use super::{bypass_enabled_in, bypass_http_followup};
    use std::path::Path;

    #[test]
    fn bypass_marker_only_enables_the_selected_port() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("azpro-bypass-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("bypass-http-ssh-80.enabled"), b"").unwrap();

        assert!(bypass_enabled_in(80, &directory));
        assert!(!bypass_enabled_in(443, &directory));

        std::fs::write(directory.join("multiprotocol.enabled"), b"").unwrap();
        assert!(!bypass_enabled_in(443, &directory));
        std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    }

    #[test]
    fn bypass_http_sends_second_101_then_200_with_configured_status() {
        assert_eq!(
            bypass_http_followup("Azpro"),
            b"HTTP/1.1 101 Azpro\r\n\r\nHTTP/1.1 200 Azpro\r\n\r\n"
        );
    }
}

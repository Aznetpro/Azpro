use std::io;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};

const MAX_PEEK: usize = 16 * 1024;
const DETECT_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Socks5,
    Tls,
    WebSocket,
    Security,
    Http,
    SshDirect,
    Unknown,
}

/// Classifica somente handshakes reconhecidos. Nenhum byte é consumido.
pub fn detect_protocol(data: &[u8]) -> Option<Protocol> {
    if data.is_empty() {
        return None;
    }

    if data[0] == 0x05 {
        return Some(Protocol::Socks5);
    }
    if data[0] == 0x16 {
        return Some(Protocol::Tls);
    }
    if starts_with_ascii_case_insensitive(data, b"SECURITY")
        || starts_with_ascii_case_insensitive(data, b"AUTH")
    {
        return Some(Protocol::Security);
    }
    if data.starts_with(b"SSH-") {
        return Some(Protocol::SshDirect);
    }

    if is_http_method_prefix(data) {
        let request = String::from_utf8_lossy(data);
        let lower = request.to_ascii_lowercase();
        if !lower.contains("\r\n\r\n") {
            return None;
        }
        if lower.contains("upgrade: security") || lower.contains("x-protocol: security") {
            return Some(Protocol::Security);
        }
        if lower.contains("upgrade: websocket") {
            return Some(Protocol::WebSocket);
        }
        return Some(Protocol::Http);
    }

    None
}

/// Aguarda um prefixo suficiente para classificação, usando peek para preservar
/// o handshake para o respectivo handler. Conexões silenciosas usam SSH direto.
pub async fn detect_from_stream(stream: &TcpStream) -> io::Result<Protocol> {
    let mut buffer = vec![0u8; MAX_PEEK];
    let mut bytes_peeked = 0usize;

    let result = timeout(DETECT_TIMEOUT, async {
        loop {
            bytes_peeked = stream.peek(&mut buffer).await?;
            if bytes_peeked == 0 {
                return Ok(Protocol::SshDirect);
            }

            if let Some(protocol) = detect_protocol(&buffer[..bytes_peeked]) {
                return Ok(protocol);
            }

            if bytes_peeked >= MAX_PEEK {
                return Ok(if is_http_method_prefix(&buffer[..bytes_peeked]) {
                    Protocol::Http
                } else {
                    Protocol::Unknown
                });
            }

            if is_http_method_prefix(&buffer[..bytes_peeked]) {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            if !may_be_incomplete_known_prefix(&buffer[..bytes_peeked]) {
                return Ok(Protocol::Unknown);
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    match result {
        Ok(classification) => classification,
        Err(_) if bytes_peeked == 0 => Ok(Protocol::SshDirect),
        Err(_) if is_http_method_prefix(&buffer[..bytes_peeked]) => Ok(Protocol::Http),
        Err(_) => Ok(Protocol::Unknown),
    }
}

fn is_http_method_prefix(data: &[u8]) -> bool {
    const RESPONSE_PREFIX: &[u8] = b"HTTP/";
    if data.len() <= RESPONSE_PREFIX.len()
        && RESPONSE_PREFIX[..data.len()].eq_ignore_ascii_case(data)
    {
        return true;
    }
    if data.len() > RESPONSE_PREFIX.len()
        && data[..RESPONSE_PREFIX.len()].eq_ignore_ascii_case(RESPONSE_PREFIX)
    {
        return true;
    }

    let method_end = data
        .iter()
        .position(|byte| *byte == b' ')
        .unwrap_or(data.len());
    let method = &data[..method_end];
    if method.is_empty() || method.len() > 32 || !method.iter().all(|byte| is_http_token(*byte)) {
        return false;
    }

    // Antes do espaço, pode ser um método dividido entre pacotes; depois dele,
    // a linha continua sendo tratada como HTTP até a detecção dos headers.
    true
}

fn is_http_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn may_be_incomplete_known_prefix(data: &[u8]) -> bool {
    const PREFIXES: [&[u8]; 13] = [
        b"SSH-",
        b"SECURITY",
        b"AUTH",
        b"GET ",
        b"POST ",
        b"PUT ",
        b"DELETE ",
        b"PATCH ",
        b"HEAD ",
        b"CONNECT ",
        b"OPTIONS ",
        b"TRACE ",
        b"HTTP/",
    ];
    PREFIXES
        .iter()
        .any(|prefix| data.len() < prefix.len() && prefix[..data.len()].eq_ignore_ascii_case(data))
}

fn starts_with_ascii_case_insensitive(data: &[u8], prefix: &[u8]) -> bool {
    data.len() >= prefix.len() && prefix.eq_ignore_ascii_case(&data[..prefix.len()])
}

#[cfg(test)]
mod tests {
    use super::{detect_protocol, Protocol};

    #[test]
    fn detects_binary_and_raw_protocol_signatures() {
        assert_eq!(detect_protocol(&[0x05, 0x01, 0x00]), Some(Protocol::Socks5));
        assert_eq!(detect_protocol(&[0x16, 0x03, 0x01]), Some(Protocol::Tls));
        assert_eq!(
            detect_protocol(b"SSH-2.0-client\r\n"),
            Some(Protocol::SshDirect)
        );
        assert_eq!(detect_protocol(b"SECURITY /\r\n"), Some(Protocol::Security));
        assert_eq!(detect_protocol(b"AUTH payload"), Some(Protocol::Security));
    }

    #[test]
    fn distinguishes_websocket_security_and_http_after_complete_headers() {
        let websocket =
            b"GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\n\r\n";
        let security = b"GET / HTTP/1.1\r\nUpgrade: security\r\nConnection: Upgrade\r\n\r\n";
        let http = b"CONNECT host:22 HTTP/1.1\r\nHost: host:22\r\n\r\n";
        assert_eq!(detect_protocol(websocket), Some(Protocol::WebSocket));
        assert_eq!(detect_protocol(security), Some(Protocol::Security));
        assert_eq!(detect_protocol(http), Some(Protocol::Http));
    }

    #[test]
    fn waits_for_partial_http_headers() {
        assert_eq!(detect_protocol(b"GET / HTTP/1.1\r\nHost: ex"), None);
    }

    #[test]
    fn detects_extension_http_methods_but_rejects_non_token_lines() {
        let propfind = b"PROPFIND /dav HTTP/1.1\r\nHost: example\r\n\r\n";
        let m_search = b"M-SEARCH * HTTP/1.1\r\nHost: example\r\n\r\n";
        assert_eq!(detect_protocol(propfind), Some(Protocol::Http));
        assert_eq!(detect_protocol(m_search), Some(Protocol::Http));
        assert_eq!(detect_protocol(b"?? / HTTP/1.1\r\n\r\n"), None);
    }
}

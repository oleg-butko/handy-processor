use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

use serde::Deserialize;

const SERVICE_ID: &str = "handy-processor";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    service: String,
    version: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProbeResult {
    Unavailable,
    Compatible,
    Incompatible(String),
}

pub fn probe(port: u16, timeout: Duration) -> ProbeResult {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = match TcpStream::connect_timeout(&address, timeout) {
        Ok(stream) => stream,
        Err(_) => return ProbeResult::Unavailable,
    };

    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return ProbeResult::Incompatible("cannot configure health connection".into());
    }

    let request =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return ProbeResult::Incompatible("port accepted but health request failed".into());
    }

    let mut response = Vec::new();
    if stream
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut response)
        .is_err()
    {
        return ProbeResult::Incompatible("port accepted but health response failed".into());
    }

    parse_response(&response)
}

fn parse_response(response: &[u8]) -> ProbeResult {
    let Some(header_end) = response.windows(4).position(|part| part == b"\r\n\r\n") else {
        return ProbeResult::Incompatible("health response is not HTTP".into());
    };
    let header = String::from_utf8_lossy(&response[..header_end]);
    let status_line = header.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        return ProbeResult::Incompatible(format!("health returned {status_line}"));
    }

    let raw_body = &response[(header_end + 4)..];
    let body = if header
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
    {
        match decode_chunked(raw_body) {
            Some(body) => body,
            None => return ProbeResult::Incompatible("health chunked body is invalid".into()),
        }
    } else {
        raw_body.to_vec()
    };
    let health: HealthResponse = match serde_json::from_slice(&body) {
        Ok(health) => health,
        Err(_) => return ProbeResult::Incompatible("health body is invalid JSON".into()),
    };

    if health.status != "ok"
        || health.service != SERVICE_ID
        || health.version != env!("CARGO_PKG_VERSION")
    {
        return ProbeResult::Incompatible(format!(
            "unexpected service identity: service={}, version={}, status={}",
            health.service, health.version, health.status
        ));
    }

    ProbeResult::Compatible
}

fn decode_chunked(mut input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    loop {
        let line_end = input.windows(2).position(|part| part == b"\r\n")?;
        let size_text = std::str::from_utf8(&input[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        input = &input[(line_end + 2)..];

        if size == 0 {
            return Some(output);
        }
        if input.len() < size + 2 || &input[size..(size + 2)] != b"\r\n" {
            return None;
        }

        output.extend_from_slice(&input[..size]);
        input = &input[(size + 2)..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};

    #[test]
    fn accepts_the_expected_service_identity() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n",
            "{\"status\":\"ok\",\"service\":\"handy-processor\",",
            "\"version\":\"0.1.0\",\"pid\":42}"
        );

        assert_eq!(parse_response(response.as_bytes()), ProbeResult::Compatible);
    }

    #[test]
    fn accepts_node_chunked_responses() {
        let body = concat!(
            "{\"status\":\"ok\",\"service\":\"handy-processor\",",
            "\"version\":\"0.1.0\",\"pid\":42}"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );

        assert_eq!(parse_response(response.as_bytes()), ProbeResult::Compatible);
    }

    #[test]
    fn rejects_an_unrelated_local_service() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n",
            "{\"status\":\"ok\",\"service\":\"something-else\",",
            "\"version\":\"0.1.0\"}"
        );

        assert!(matches!(
            parse_response(response.as_bytes()),
            ProbeResult::Incompatible(_)
        ));
    }

    #[test]
    fn probes_a_real_local_http_listener() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = connection.read(&mut request).unwrap();
            let body = format!(
                "{{\"status\":\"ok\",\"service\":\"{SERVICE_ID}\",\"version\":\"{}\"}}",
                env!("CARGO_PKG_VERSION")
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            connection.write_all(response.as_bytes()).unwrap();
        });

        assert_eq!(probe(port, Duration::from_secs(1)), ProbeResult::Compatible);
        server.join().unwrap();
    }
}

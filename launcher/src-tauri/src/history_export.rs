use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};

const IO_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_HEADER_BYTES: usize = 64 * 1024;
const EXPORT_FILE_PREFIX: &str = "handy-processor-history-";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportFormat {
    Text,
    Jsonl,
}

impl ExportFormat {
    fn request_path(self) -> &'static str {
        match self {
            Self::Text => "/api/transformations/export",
            Self::Jsonl => "/api/transformations/export?format=jsonl",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Text => ".txt",
            Self::Jsonl => ".jsonl",
        }
    }
}

pub fn download_history_export(
    directory: &Path,
    port: u16,
    format: ExportFormat,
) -> Result<PathBuf> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = TcpStream::connect_timeout(&address, IO_TIMEOUT)
        .with_context(|| format!("cannot connect to history server on port {port}"))?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
        format.request_path()
    )?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let headers = read_response_headers(&mut reader)?;
    let file_name = export_file_name(&headers.content_disposition, format)?;
    let (path, mut file) = create_export_file(directory, file_name, format)?;

    let result = if headers.chunked {
        copy_chunked(&mut reader, &mut file)
    } else {
        io::copy(&mut reader, &mut file).map(|_| ())
    };
    if let Err(error) = result.and_then(|()| file.flush()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error).with_context(|| format!("cannot save {}", path.display()));
    }

    Ok(path)
}

struct ResponseHeaders {
    chunked: bool,
    content_disposition: String,
}

fn read_response_headers(reader: &mut impl BufRead) -> Result<ResponseHeaders> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status = line.trim_end_matches(['\r', '\n']);
    if !status.starts_with("HTTP/1.1 200 ") && !status.starts_with("HTTP/1.0 200 ") {
        bail!("history export returned {status}");
    }

    let mut total_bytes = line.len();
    let mut chunked = false;
    let mut content_disposition = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            bail!("history export response ended before its headers");
        }
        total_bytes += line.len();
        if total_bytes > MAX_HEADER_BYTES {
            bail!("history export response headers are too large");
        }
        if line == "\r\n" || line == "\n" {
            break;
        }

        let Some((name, value)) = line.split_once(':') else {
            bail!("history export response contains an invalid header");
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"));
        } else if name.eq_ignore_ascii_case("content-disposition") {
            content_disposition = Some(value.to_owned());
        }
    }

    Ok(ResponseHeaders {
        chunked,
        content_disposition: content_disposition
            .context("history export response has no Content-Disposition header")?,
    })
}

fn export_file_name(content_disposition: &str, format: ExportFormat) -> Result<&str> {
    let file_name = content_disposition
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("filename="))
        .map(|value| value.trim_matches('"'))
        .context("history export response has no filename")?;
    let date = file_name
        .strip_prefix(EXPORT_FILE_PREFIX)
        .and_then(|value| value.strip_suffix(format.suffix()))
        .context("history export filename is invalid")?;
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, value)| index != 4 && index != 7 && !value.is_ascii_digit())
    {
        bail!("history export filename is invalid");
    }
    Ok(file_name)
}

fn create_export_file(
    directory: &Path,
    file_name: &str,
    format: ExportFormat,
) -> Result<(PathBuf, File)> {
    let stem = file_name
        .strip_suffix(format.suffix())
        .context("history export filename has no extension")?;
    for index in 1..=10_000 {
        let candidate_name = if index == 1 {
            file_name.to_owned()
        } else {
            format!("{stem}-{index}{}", format.suffix())
        };
        let path = directory.join(candidate_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("cannot create {}", path.display()))
            }
        }
    }
    bail!("too many history export files already exist")
}

fn copy_chunked(reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "chunk size is missing",
            ));
        }
        let size_text = line
            .trim_end_matches(['\r', '\n'])
            .split(';')
            .next()
            .unwrap_or_default();
        let size = u64::from_str_radix(size_text, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid chunk size"))?;
        if size == 0 {
            loop {
                line.clear();
                reader.read_line(&mut line)?;
                if line == "\r\n" || line == "\n" || line.is_empty() {
                    return Ok(());
                }
            }
        }

        let copied = io::copy(&mut Read::by_ref(reader).take(size), writer)?;
        if copied != size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "chunk body is incomplete",
            ));
        }
        let mut ending = [0_u8; 2];
        reader.read_exact(&mut ending)?;
        if ending != *b"\r\n" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chunk ending is invalid",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread, time::SystemTime};

    fn temporary_directory() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "handy-processor-export-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn serve_export(
        listener: TcpListener,
        body: &'static str,
        format: ExportFormat,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut request_line = String::new();
            BufReader::new(&connection)
                .read_line(&mut request_line)
                .unwrap();
            assert_eq!(
                request_line.trim_end_matches(['\r', '\n']),
                format!("GET {} HTTP/1.1", format.request_path())
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=\"handy-processor-history-2026-08-30{}\"\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
                format.suffix(), body.len()
            );
            connection.write_all(response.as_bytes()).unwrap();
        })
    }

    #[test]
    fn downloads_chunked_export_without_overwriting_an_existing_file() {
        let directory = temporary_directory();
        fs::write(
            directory.join("handy-processor-history-2026-08-30.txt"),
            "existing",
        )
        .unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = serve_export(listener, "исходный\nрезультат\n\n", ExportFormat::Text);

        let path = download_history_export(&directory, port, ExportFormat::Text).unwrap();

        assert_eq!(
            path.file_name().unwrap(),
            "handy-processor-history-2026-08-30-2.txt"
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "исходный\nрезультат\n\n"
        );
        assert_eq!(
            fs::read_to_string(directory.join("handy-processor-history-2026-08-30.txt")).unwrap(),
            "existing"
        );
        server.join().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn downloads_jsonl_with_its_own_extension_and_request_path() {
        let directory = temporary_directory();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = "{\"originalText\":\"свод\",\"transformedText\":\"слот\"}\n";
        let server = serve_export(listener, body, ExportFormat::Jsonl);

        let path = download_history_export(&directory, port, ExportFormat::Jsonl).unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            "handy-processor-history-2026-08-30.jsonl"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), body);
        server.join().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_unsafe_export_filenames() {
        assert!(
            export_file_name("attachment; filename=\"../history.txt\"", ExportFormat::Text).is_err()
        );
        assert!(
            export_file_name("attachment; filename=\"history.txt\"", ExportFormat::Text).is_err()
        );
        assert!(export_file_name(
            "attachment; filename=\"handy-processor-history-2026-08-30.txt\"",
            ExportFormat::Jsonl
        )
        .is_err());
    }
}

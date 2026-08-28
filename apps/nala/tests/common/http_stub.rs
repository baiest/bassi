use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

/// A minimal single-request HTTP stub server, used to test `OllamaLlm`
/// against a real socket without needing a live Ollama instance or an
/// extra HTTP-mocking dependency.
///
/// Serves exactly one request on a background thread, then shuts down.
pub struct HttpStub {
    pub base_url: String,
    request_body: mpsc::Receiver<String>,
}

impl HttpStub {
    pub fn start(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind stub listener");
        let port = listener
            .local_addr()
            .expect("failed to read local addr")
            .port();

        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("failed to accept connection");
            let request_body = read_request_body(&stream);
            let _ = tx.send(request_body);

            respond(stream, status, body);
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            request_body: rx,
        }
    }

    pub fn received_body(&self) -> String {
        self.request_body
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("stub did not receive a request in time")
    }
}

fn read_request_body(stream: &std::net::TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut content_length = 0usize;

    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("failed to read header line");

        let line = line.trim_end();
        if line.is_empty() {
            break;
        }

        if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    reader
        .read_exact(&mut body)
        .expect("failed to read request body");

    String::from_utf8(body).expect("request body was not valid UTF-8")
}

fn respond(mut stream: std::net::TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    stream
        .write_all(response.as_bytes())
        .expect("failed to write stub response");
}

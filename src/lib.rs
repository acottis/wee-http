mod http;
pub use http::{Method, Request, Response, StatusCode};

pub type Handler = fn(Request) -> Response;

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::Arc,
    thread,
    time::Duration,
};

pub struct Server;

impl Server {
    pub fn bind(addr: impl ToSocketAddrs) -> ServerBuilder {
        ServerBuilder {
            listener: TcpListener::bind(addr).unwrap(),
            paths: HashMap::new(),
            default: not_found,
        }
    }
}
pub struct ServerBuilder {
    listener: TcpListener,
    paths: HashMap<String, Handler>,
    default: Handler,
}

impl ServerBuilder {
    pub fn path(mut self, path: &str, handler: Handler) -> Self {
        self.paths
            .insert(path.trim_end_matches('/').into(), handler);
        self
    }

    pub fn listen(self) {
        let paths = Arc::new(self.paths);

        for stream in self.listener.incoming() {
            let paths_clone = paths.clone();
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        Self::handle(stream, paths_clone, self.default)
                    });
                }
                Err(err) => println!("{err:?}"),
            };
        }
    }

    /// The default response the web server will serve if their is no matching path
    pub fn default(mut self, handler: Handler) -> Self {
        self.default = handler;
        self
    }

    fn handle(
        mut stream: TcpStream,
        paths: Arc<HashMap<String, Handler>>,
        default: Handler,
    ) {
        println!("{stream:?}");
        set_stream_timeouts(&stream, Duration::from_millis(1000));

        let mut recv_buf = [0u8; u16::MAX as usize];
        let len = stream.read(&mut recv_buf).unwrap();
        let request = Request::from_bytes(&recv_buf[..len]);
        println!("{request:?}");

        let mut response: Response = match paths.get(request.path()) {
            Some(handler) => handler(request),
            None => default(request),
        };

        stream.write(response.serialise().as_bytes()).unwrap();
    }
}

fn set_stream_timeouts(stream: &TcpStream, duration: Duration) {
    stream.set_read_timeout(Some(duration)).unwrap();
    stream.set_write_timeout(Some(duration)).unwrap();
}

fn not_found(_: Request) -> Response {
    Response::new()
        .set_status_code(http::StatusCode::NotFound)
        .set_body("404 Not Found\nOops! Looks like Nessie took our page for a swim in the Loch")
}

#[cfg(feature = "tls")]
use rustls::ServerConfig;

#[cfg(feature = "tls")]
use std::{fs::File, io::BufReader, path::Path};

#[cfg(feature = "tls")]
pub struct TlsServer;

#[cfg(feature = "tls")]
impl TlsServer {
    pub fn bind(
        addr: impl ToSocketAddrs,
        private_key: impl AsRef<Path>,
        certs: impl AsRef<Path>,
    ) -> TlsServerBuilder {
        let certs = rustls_pemfile::certs(&mut BufReader::new(
            &mut File::open(certs).unwrap(),
        ))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

        let private_key = rustls_pemfile::private_key(&mut BufReader::new(
            &mut File::open(private_key).unwrap(),
        ))
        .unwrap()
        .unwrap();

        TlsServerBuilder {
            listener: TcpListener::bind(addr).unwrap(),
            tls_config: ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, private_key)
                .unwrap(),
            paths: HashMap::new(),
        }
    }
}

#[cfg(feature = "tls")]
pub struct TlsServerBuilder {
    listener: TcpListener,
    tls_config: ServerConfig,
    paths: HashMap<String, Handler>,
}

#[cfg(feature = "tls")]
impl TlsServerBuilder {
    pub fn path(mut self, path: &str, handler: Handler) -> Self {
        self.paths
            .insert(path.trim_end_matches('/').into(), handler);
        self
    }

    pub fn listen(self) {
        let tls_config = Arc::new(self.tls_config);
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let tls_config_clone = tls_config.clone();
                    thread::spawn(move || {
                        Self::handle_tls(stream, tls_config_clone)
                    });
                }
                Err(err) => println!("{err:?}"),
            };
        }
    }

    fn handle_tls(mut stream: TcpStream, tls_config: Arc<ServerConfig>) {
        println!("{stream:?}");
        set_stream_timeouts(&stream, Duration::from_millis(1000));

        let mut conn = rustls::ServerConnection::new(tls_config).unwrap();
        conn.complete_io(&mut stream).unwrap();

        conn.read_tls(&mut stream).unwrap();
        conn.process_new_packets().unwrap();
        let mut recv_buf = [0u8; u16::MAX as usize];
        let _ = conn.reader().read(&mut recv_buf).unwrap();

        conn.writer()
            .write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes())
            .unwrap();
        conn.write_tls(&mut stream).unwrap();
        conn.process_new_packets().unwrap();
    }
}

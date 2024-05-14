mod http;
pub use http::{Method, Request, Response};

pub type Handler = fn(Request) -> Response;

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::Arc,
    thread,
    time::Duration,
};

#[cfg(feature = "log")]
use log::info;

#[cfg(feature = "tls")]
use rustls::{ServerConfig, ServerConnection};
#[cfg(feature = "tls")]
use std::{fs::File, io::BufReader, path::Path};

pub struct Server {
    listener: TcpListener,
    #[cfg(feature = "tls")]
    tls_config: Option<ServerConfig>,
    paths: HashMap<String, Handler>,
}

impl Server {
    pub fn bind(addr: impl ToSocketAddrs) -> Self {
        Self {
            listener: TcpListener::bind(addr).unwrap(),
            #[cfg(feature = "tls")]
            tls_config: None,
            paths: HashMap::new(),
        }
    }

    pub fn path(mut self, path: &str, handler: Handler) -> Self {
        self.paths
            .insert(path.trim_end_matches('/').into(), handler);
        self
    }

    #[cfg(feature = "tls")]
    pub fn tls(
        mut self,
        private_key: impl AsRef<Path>,
        certs: impl AsRef<Path>,
    ) -> Self {
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

        self.tls_config = Some(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, private_key)
                .unwrap(),
        );
        self
    }

    pub fn listen(self) {
        let paths = Arc::new(self.paths);

        #[cfg(not(feature = "tls"))]
        for stream in self.listener.incoming() {
            let paths_clone = paths.clone();
            match stream {
                Ok(stream) => {
                    thread::spawn(move || handle(stream, paths_clone));
                }
                Err(err) => println!("{err:?}"),
            };
        }

        #[cfg(feature = "tls")]
        match self.tls_config {
            Some(tls_config) => {
                let tls_config = Arc::new(tls_config);
                for stream in self.listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let tls_config_clone = tls_config.clone();
                            thread::spawn(move || {
                                handle_tls(stream, tls_config_clone)
                            });
                        }
                        Err(err) => println!("{err:?}"),
                    };
                }
            }
            None => {
                for stream in self.listener.incoming() {
                    let paths_clone = paths.clone();
                    match stream {
                        Ok(stream) => {
                            thread::spawn(move || handle(stream, paths_clone));
                        }
                        Err(err) => println!("{err:?}"),
                    };
                }
            }
        }
    }
}

fn set_stream_timeouts(stream: &TcpStream, duration: Duration) {
    stream.set_read_timeout(Some(duration)).unwrap();
    stream.set_write_timeout(Some(duration)).unwrap();
}

fn handle(mut stream: TcpStream, paths: Arc<HashMap<String, Handler>>) {
    println!("{stream:?}");
    set_stream_timeouts(&stream, Duration::from_millis(1000));

    let mut recv_buf = [0u8; 2048];
    let len = stream.read(&mut recv_buf).unwrap();
    let request = Request::from_bytes(&recv_buf[..len]);
    println!("{request:?}");

    let mut response: Response = match paths.get(request.path()) {
        Some(handler) => handler(request),
        None => not_found(),
    };

    stream.write(response.serialise().as_bytes()).unwrap();
}

fn not_found() -> Response {
    Response::new()
        .set_status_code(http::StatusCode::NotFound)
        .set_body("404 Not Found\nOops! Looks like Nessie took our page for a swim in the Loch")
}

#[cfg(feature = "tls")]
fn handle_tls(mut stream: TcpStream, tls_config: Arc<ServerConfig>) {
    println!("{stream:?}");
    set_stream_timeouts(&stream, Duration::from_millis(1000));

    let mut conn = ServerConnection::new(tls_config).unwrap();
    conn.complete_io(&mut stream).unwrap();

    conn.read_tls(&mut stream).unwrap();
    conn.process_new_packets().unwrap();
    let mut recv_buf = [0u8; 1024];
    let _ = conn.reader().read(&mut recv_buf).unwrap();

    conn.writer()
        .write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes())
        .unwrap();
    conn.write_tls(&mut stream).unwrap();
    conn.process_new_packets().unwrap();
}

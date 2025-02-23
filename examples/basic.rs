use wee_http::{Request, Response, Server};

fn main() {
    Server::bind("0.0.0.0:8080").path("/", root).listen()
}

fn root(req: Request) -> Response {
    let res = Response::new();
    res
}

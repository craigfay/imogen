
use imogen::ImageServer;
use std::env;

fn main() {
    let port = env::args().nth(1)
        .unwrap_or("8080".to_string()).parse::<u64>()
        .expect("Invalid port provided");

    let uploads_dir = env::args().nth(2)
        .unwrap_or("./uploads".to_string());

    ImageServer::listen(port, uploads_dir);
}

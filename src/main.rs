extern crate core;

use std::{env};
use my_http_server::{Config, Server};

fn main() {
    let args: Vec<String> = env::args().collect();

    let config = Config::new(&args).unwrap_or_else(|err| panic!("{}", err));
    let server = Server::new(config);

    server.start();
}


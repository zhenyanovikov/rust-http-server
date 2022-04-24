use std::collections::HashMap;
use std::error::Error;
use std::{fs, thread};
use std::fs::{ReadDir};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use chrono::Local;
use http::StatusCode;

pub struct Config {
    pub host: String,
    pub root: String,
}

impl Config {
    pub fn new(args: &[String]) -> Result<Self, String> {
        if args.len() < 3 {
            return Err(String::from("not enough arguments"));
        }

        Ok(Config {
            host: String::from(args.get(1).unwrap()),
            root: String::from(args.get(2).unwrap()),
        })
    }
}

pub struct Server {
    config: Config,
}

struct Request {
    request_input: TcpStream,

    path: String,
    method: String,
    headers: HashMap<String, String>,

    response_code: u16,
}

impl Server {
    pub fn new(config: Config) -> Server {
        Server {
            config
        }
    }

    pub fn start(self) {
        let listener = TcpListener::bind(&self.config.host).unwrap();
        let addr = listener.local_addr().unwrap();

        log(&format!("Starting http server on {}:{}", addr.ip(), addr.port()));

        let arc = Arc::new(self);
        for stream in listener.incoming() {
            let server = Arc::clone(&arc);
            thread::spawn(move || {
                // log("thread spawned");
                let mut stream = stream.unwrap();
                loop {
                    let mut req = Request {
                        request_input: stream,
                        path: String::new(),
                        method: String::new(),
                        headers: HashMap::new(),
                        response_code: 0,
                    };
                    if let Err(err) = server.handle_connection(&mut req) {
                        log(&format!("Got error: {}", err));
                        return;
                    }
                    if req.method == "" {
                        break;
                    }

                    log(&format!("{:<6} {:<35} {}",
                                 req.method, req.path, StatusCode::from_u16(req.response_code).unwrap()));

                    stream = req.request_input;
                }
                // log("thread died");
            });
        }
    }

    fn handle_connection(&self, req: &mut Request) -> Result<(), Box<dyn Error>> {
        if let Err(_) = req.parse_request() {
            req.response(StatusCode::BAD_REQUEST.as_u16(), Vec::new())?;
            return Ok(());
        };

        if req.method == "" {
            return Ok(());
        }

        if !(req.method == "GET") {
            req.response(StatusCode::NOT_IMPLEMENTED.as_u16(), Vec::from("Not Implemented"))?;
            return Ok(());
        }

        match fs::metadata(&req.path) {
            Ok(metadata) => {
                if metadata.is_dir() {
                    let dir = fs::read_dir(&req.path).unwrap();

                    req.response(StatusCode::OK.as_u16(), dir_navigation_page(&req.path,dir))?;
                } else if metadata.is_file() {
                    match fs::read(&req.path) {
                        Ok(content) => {
                            req.response(StatusCode::OK.as_u16(), content)?;
                        }
                        Err(_) => {
                            req.response(StatusCode::NOT_FOUND.as_u16(), Vec::from("Not Found"))?;
                        }
                    };
                }
            }
            Err(_) => {
                req.response(StatusCode::NOT_FOUND.as_u16(), Vec::from("Not Found"))?;
            }
        }


        return Ok(());
    }
}

fn dir_navigation_page(dir_name: &String, dir: ReadDir) -> Vec<u8> {
    let mut file_list = String::new();
    dir.into_iter().for_each(|f| {
        let file = f.unwrap();
        file_list.push_str(&format!("<a href=\"{}\">{}</a><br>", file.path().into_os_string().into_string().unwrap(),
                                    file.file_name().to_str().unwrap()));
    });

    format!("<html><body><h1>{}</h1><br>{}</body></html>", dir_name, file_list).into_bytes()
}

impl Request {
    fn parse_request(&mut self) -> Result<(), Box<dyn Error>> {
        let mut buffer = [0; 1024];

        self.request_input.read(&mut buffer)?;

        let buffer = String::from_utf8(Vec::from(buffer))?;
        if buffer == "" {
            return Ok(());
        }

        let mut lines = buffer.lines();

        if let Some(first_line) = lines.next() {
            let words: Vec<&str> = first_line.split(" ").collect();
            if words.len() != 3 {
                return Ok(());
            }

            self.method.push_str(words[0]);
            self.path.push_str(words[1]);
            let _version = words[2];
        } else {
            return Ok(());
        }

        for line in lines {
            let parts: Vec<&str> = line.splitn(2, ":").collect();
            if parts.len() != 2 {
                continue;
            }

            self.headers.insert(String::from(parts[0]),
                                String::from(parts[1]).trim().to_string());
        }

        Ok(())
    }

    fn response(&mut self, code: u16, data: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let mut headers = HashMap::new();

        headers.insert("Connection", "keep-alive");

        // let len = data.len().to_string();
        // headers.insert("Content-Length", len.as_str());
        headers.insert("Transfer-Encoding", "chunked");

        let headers = headers.iter().
            fold(String::new(),
                 |s, kv| {
                     format!("{}\n{}: {}", s, kv.0, kv.1)
                 });

        let response = Vec::from(format!("HTTP/1.1 {}{}\n\n",
                                         StatusCode::from_u16(code)?,
                                         headers));

        self.request_input.write(&response)?;


        const BYTES_PER_CHUNK: usize = 500;

        let data = data.as_slice();
        let mut chunks = data.len() / BYTES_PER_CHUNK;
        if data.len() % BYTES_PER_CHUNK > 0 {
            chunks += 1;
        }

        for chunk_num in 0..chunks {
            let in_chunk_bytes = if chunk_num == chunks - 1 {
                data.len() % BYTES_PER_CHUNK
            } else {
                BYTES_PER_CHUNK
            };

            let offset = chunk_num * BYTES_PER_CHUNK;
            let chunk = &data[offset..offset + in_chunk_bytes];
            self.request_input.write(format!("{:x}", in_chunk_bytes).to_string().as_bytes())?;
            self.request_input.write(b"\r\n")?;
            self.request_input.write(chunk)?;
            self.request_input.write(b"\r\n")?;
        }
        self.request_input.write(b"0\r\n\r\n")?;
        self.request_input.flush()?;

        self.response_code = code;

        Ok(())
    }
}

fn log(message: &str) {
    let date = Local::now();
    println!("[{}]: {}", date.format("%H:%M:%S"), message);
}

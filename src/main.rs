mod open_explorer;
use crate::open_explorer::open;

use actix_files::Files;
use actix_web::{App, HttpServer};
use qrcode::QrCode;

use std::{io::Result, net::IpAddr};

#[actix_web::main]
async fn main() -> Result<()> {
    println!("Live Server");
    let ip = match get_local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "0.0.0.0".to_string(),
    };
    let port = get_available_port();
    let server = HttpServer::new(|| {
        App::new().service(
            Files::new("/", ".")
                .show_files_listing()
                .index_file("index.html"),
        )
    })
    .bind((ip.clone(), port))?;
    let url = format!("http://{}:{}", ip, port);
    open(&url);
    println!("Started at {}", &url);
    match QrCode::new(&url) {
        Ok(code) => {
            let code_string = code
                .render::<char>()
                .quiet_zone(false)
                .module_dimensions(2, 1)
                .build();
            println!("{}", code_string);
        }
        Err(_) => (),
    };
    server.run().await
}

fn get_available_port() -> u16 {
    std::net::TcpListener::bind("0.0.0.0:0")
        .map(|listener| listener.local_addr().unwrap().port())
        .unwrap_or(0)
}

fn get_local_ip() -> Result<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let local_ip = socket.local_addr()?.ip();
    Ok(local_ip)
}

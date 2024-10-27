mod open_explorer;
use crate::open_explorer::open;

use actix_web::{
    get,
    http::{
        header::{self, DispositionParam, DispositionType},
        StatusCode,
    },
    web, App, HttpResponse, HttpServer, Responder,
};
use chrono::{FixedOffset, TimeZone, Utc};
use colored::Colorize;
use futures::stream::{self};
use mime_guess::mime::{CSS, HTML, IMAGE, JAVASCRIPT, JSON, TEXT, XML};
use qrcode::{Color, QrCode};
use serde::Serialize;
use tera::{Context, Tera};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
};
use unicode_width::UnicodeWidthStr;
use walkdir::WalkDir;

use std::{
    cmp::Ordering,
    env,
    io::{ErrorKind, Result},
    net::IpAddr,
    path::PathBuf,
    time::SystemTime,
};

lazy_static::lazy_static! {
    static ref tmpl: Tera = {
        let mut tera = Tera::default();
        tera.add_raw_template("index.html", include_str!("../index.html.tera"))
            .expect("Failed to add index.html template");
        tera
    };
    static ref current_dir: PathBuf = env::current_dir().unwrap();
}

#[derive(Serialize, Debug)]
struct IndexChild {
    name: String,
    file_type: String,
    last_modified: String,
    size: String,
    path: String,
}

#[actix_web::main]
async fn main() -> Result<()> {
    let ip = match get_local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "0.0.0.0".to_string(),
    };
    let mut port: u16 = 5437;
    let server = loop {
        let server = HttpServer::new(|| App::new().service(handler));
        match server.bind(format!("{}:{}", ip, port)) {
            Ok(server) => break server,
            Err(_) => {
                port = get_available_port();
            }
        }
    };
    let url = format!("http://{}:{}", ip, port);
    open(&url);
    echo_renderer(url.clone());
    server.run().await
}

fn echo_renderer(url: String) {
    let code = QrCode::new(&url).unwrap();
    let width = code.width();
    let colors: Vec<Vec<qrcode::Color>> = code
        .into_colors()
        .chunks(width)
        .map(|x| x.to_vec())
        .collect();

    let output: Vec<String> = colors
        .chunks(2)
        .map(|chunk| {
            let (line1, line2) = match chunk {
                [line1, line2] => (line1, Some(line2)),
                [line1] => (line1, None),
                _ => unreachable!(),
            };

            let mut line = String::new();
            let line_len = line1.len();

            for i in 0..line_len {
                match line2 {
                    Some(line2) => {
                        if line1[i] == line2[i] {
                            line.push(match line1[i] {
                                Color::Light => ' ',
                                Color::Dark => '█',
                            });
                        } else {
                            line.push(match (line1[i], line2[i]) {
                                (Color::Light, Color::Dark) => '▄',
                                (Color::Dark, Color::Light) => '▀',
                                _ => ' ',
                            });
                        }
                    }
                    None => line.push(match line1[i] {
                        Color::Light => ' ',
                        Color::Dark => '▀',
                    }),
                }
            }
            line
        })
        .collect();

    let height = output.len() as u16;
    let term_width = termsize::get().map(|size| size.cols).unwrap_or(80);

    let logo = r" _     _           ____ 
| |   (_)_   _____/ ___|  ___ _ ____   _____ _ __ 
| |   | \ \ / / _ \___ \ / _ \ '__\ \ / / _ \ '__| 
| |___| |\ V /  __/___) |  __/ |   \ V /  __/ | 
|_____|_| \_/ \___|____/ \___|_|    \_/ \___|_| ";

    let mut logo_lines: Vec<String> = logo.lines().map(|line| line.to_string()).collect();
    logo_lines.push("".to_string());
    logo_lines.push(
        format!("Started at {}", &url)
            .bright_yellow()
            .bold()
            .to_string(),
    );
    logo_lines.push(
        "Scan the QR Code to access on mobile devices"
            .bright_purple()
            .bold()
            .to_string(),
    );

    let normal_output_width = width as usize
        + 4
        + logo_lines
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or(0);

    let simple_output = vec![
        "Live Server".bright_green().bold().to_string(),
        format!("Started at {}", &url)
            .bright_yellow()
            .bold()
            .to_string(),
        "Scan the QR Code to access on mobile devices"
            .bright_purple()
            .bold()
            .to_string(),
    ];

    if term_width < width as u16 || term_width <= normal_output_width as u16 {
        for line in &simple_output {
            println!("{}", line);
        }
        for line in &output {
            println!("{}", line);
        }
        return;
    }

    let logo_height = logo_lines.len() as u16;
    let offset = ((height - logo_height) / 2) as usize;

    for i in 0..height as usize {
        if i < offset || i >= offset + logo_lines.len() {
            println!("{}", output[i]);
        } else {
            let logo_index = i - offset;
            if logo_index < logo_lines.len() {
                println!(
                    "{} {} {}",
                    output[i],
                    "|".bright_cyan().bold(),
                    logo_lines[logo_index].bright_green().bold()
                );
            } else {
                println!("{}", output[i]);
            }
        }
    }
}

#[get("/{url:.*}")]
async fn handler(path: web::Path<String>) -> impl Responder {
    let path = current_dir.join(&*path);
    if !path.exists() {
        return error_message(ErrorKind::NotFound);
    };
    if path.is_dir() {
        dir_handler(path).await
    } else if path.is_file() {
        file_handler(path).await
    } else {
        error_message(ErrorKind::Unsupported)
    }
}

async fn dir_handler(path: PathBuf) -> HttpResponse {
    let stripped_path = match path.strip_prefix(&*current_dir) {
        Ok(path) => path,
        Err(_) => return error_message(ErrorKind::PermissionDenied),
    };
    let current_path = stripped_path;

    let dir = WalkDir::new(&path)
        .max_depth(1)
        .min_depth(1)
        .sort_by(|a, b| {
            let a_type = a.file_type();
            let b_type = b.file_type();
            if a_type.is_dir() && !b_type.is_dir() {
                Ordering::Less
            } else if !a_type.is_dir() && b_type.is_dir() {
                Ordering::Greater
            } else {
                a.file_name().cmp(b.file_name())
            }
        });
    let mut children: Vec<IndexChild> = Vec::new();
    if path != PathBuf::from(current_dir.clone()) {
        children.push(IndexChild {
            name: "Go Back".to_string(),
            file_type: "back".to_string(),
            last_modified: "".to_string(),
            size: "".to_string(),
            path: format!(
                "/{}",
                path.parent()
                    .unwrap()
                    .strip_prefix(&*current_dir)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            ),
        });
    }
    for entry in dir {
        match entry {
            Ok(entry) => {
                let name = if let Some(name) = entry.file_name().to_str() {
                    if ["index.html", "index.htm"].contains(&name) {
                        return file_handler(entry.path().to_path_buf()).await;
                    };
                    name.to_string()
                } else {
                    "".to_string()
                };
                let file_type = entry.file_type();
                let size = if file_type.is_dir() {
                    "".to_string()
                } else if let Ok(metadata) = entry.metadata() {
                    format_size(metadata.len())
                } else {
                    "".to_string()
                };
                let file_type = if file_type.is_dir() {
                    "folder".to_string()
                } else if file_type.is_file() {
                    if let Some(mime) = mime_guess::from_path(entry.path()).first() {
                        if mime.type_() == IMAGE {
                            "image".to_string()
                        } else if [JAVASCRIPT, CSS, HTML, XML, JSON].contains(&mime.type_()) {
                            "lambda".to_string()
                        } else {
                            "file".to_string()
                        }
                    } else {
                        "file".to_string()
                    }
                } else {
                    "file".to_string()
                };
                let last_modified = if let Ok(metadata) = entry.metadata() {
                    format_time(metadata.modified().ok())
                } else {
                    "".to_string()
                };
                let path = format!(
                    "/{}",
                    entry
                        .path()
                        .strip_prefix(&*current_dir)
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                );

                children.push(IndexChild {
                    name,
                    file_type,
                    last_modified,
                    size,
                    path,
                });
            }
            Err(_) => (),
        }
    }
    let mut ctx = Context::new();
    ctx.insert("files", &children);
    ctx.insert("path", &current_path.to_string_lossy().to_string());

    match tmpl.render("index.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(_) => HttpResponse::InternalServerError().body("Error rendering template"),
    }
}

async fn file_handler(path: PathBuf) -> HttpResponse {
    let file = match File::open(&path).await {
        Ok(file) => file,
        Err(e) => {
            return error_message(e.kind());
        }
    };
    let file_stream = stream::unfold(file, move |mut file| async {
        let chunk_size = 1024;
        let mut buffer = vec![0; chunk_size];
        match file.read(&mut buffer).await {
            Ok(0) => None,
            Ok(n) => {
                buffer.truncate(n);
                Some((Ok::<_, io::Error>(web::Bytes::from(buffer)), file))
            }
            Err(e) => Some((Err(e), file)),
        }
    });

    let mut response_builder = HttpResponse::Ok();
    let response = response_builder.keep_alive();
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let mime = format!(
        "{}{}",
        mime.essence_str(),
        if mime.type_() == TEXT {
            "; charset=utf-8"
        } else {
            ""
        }
    );
    response.content_type(mime);
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        response.append_header(header::ContentDisposition {
            disposition: DispositionType::Inline,
            parameters: vec![DispositionParam::Filename(file_name.to_string())],
        });
    }
    if let Ok(metadata) = path.metadata() {
        response.append_header(header::ContentLength(metadata.len() as usize));
    }
    response.streaming(file_stream)
}

fn format_time(time: Option<SystemTime>) -> String {
    time.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| {
            let utc_datetime = Utc
                .timestamp_opt(duration.as_secs() as i64, 0)
                .single()
                .unwrap_or_else(Utc::now);
            let utc_plus_8 = FixedOffset::east_opt(8 * 3600).unwrap();
            let china_time = utc_datetime.with_timezone(&utc_plus_8);
            china_time.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| "".to_string())
}

fn format_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let index = (size as f64).log(1024.0).floor() as usize;
    let formatted_size = size as f64 / 1024.0f64.powi(index as i32);
    if index >= UNITS.len() {
        format!("{:.2} {}", size as f64 / 1024.0f64.powi(4), UNITS[4])
    } else {
        if formatted_size.fract() == 0.0 {
            format!("{:.0} {}", formatted_size, UNITS[index])
        } else {
            format!("{:.2} {}", formatted_size, UNITS[index])
        }
    }
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

fn error_message(e: ErrorKind) -> HttpResponse {
    let status = match e {
        ErrorKind::NotFound => ("Not Found", 404),
        ErrorKind::PermissionDenied => ("Permission Denied", 403),
        ErrorKind::ConnectionRefused => ("Connection Refused", 502),
        ErrorKind::ConnectionReset => ("Connection Reset", 503),
        ErrorKind::ConnectionAborted => ("Connection Aborted", 503),
        ErrorKind::NotConnected => ("Not Connected", 503),
        ErrorKind::AddrInUse => ("Address In Use", 409),
        ErrorKind::AddrNotAvailable => ("Address Not Available", 404),
        ErrorKind::BrokenPipe => ("Broken Pipe", 500),
        ErrorKind::AlreadyExists => ("Already Exists", 409),
        ErrorKind::WouldBlock => ("Operation Would Block", 403),
        ErrorKind::InvalidInput => ("Invalid Input", 400),
        ErrorKind::InvalidData => ("Invalid Data", 422),
        ErrorKind::TimedOut => ("Timed Out", 504),
        ErrorKind::WriteZero => ("Write Zero", 500),
        ErrorKind::Interrupted => ("Operation Interrupted", 500),
        ErrorKind::Unsupported => ("Unsupported", 501),
        ErrorKind::UnexpectedEof => ("Unexpected EOF", 500),
        ErrorKind::OutOfMemory => ("Out Of Memory", 500),
        ErrorKind::Other => ("Other Error", 500),
        _ => ("Unknown Error", 500),
    };
    HttpResponse::build(StatusCode::from_u16(status.1).unwrap()).body(status.0)
}

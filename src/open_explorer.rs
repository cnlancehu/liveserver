use std::env::{self, consts::OS};
use std::process::Command;

pub fn open(url: &str) {
    let mut cmd = match OS {
        "windows" => {
            let mut command = Command::new("rundll32");
            command.arg("url.dll,FileProtocolHandler").arg(url);
            command
        }
        "linux" => {
            // if termux then return
            match env::var("PREFIX") {
                Ok(env) => {
                    if env == "/data/data/com.termux/files/usr" {
                        return;
                    }
                }
                Err(_) => (),
            }

            let mut command = Command::new("xdg-open");
            command.arg(url);
            command
        }
        "macos" => {
            let mut command = Command::new("open");
            command.arg(url);
            command
        }
        _ => return,
    };
    let _ = cmd.spawn();
}

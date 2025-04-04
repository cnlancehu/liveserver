#[cfg(target_os = "windows")]
pub fn open(url: &str) {
    use std::process::Command;

    let mut command = Command::new("rundll32");
    command.arg("url.dll,FileProtocolHandler").arg(url);
    let _ = command.spawn();
}

#[cfg(target_os = "linux")]
pub fn open(url: &str) {
    use std::{env, process::Command};

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
    let _ = command.spawn();
}

#[cfg(target_os = "macos")]
pub fn open(url: &str) {
    use std::process::Command;

    let mut command = Command::new("open");
    command.arg(url);
    let _ = command.spawn();
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "macos"
)))]
pub fn open(_url: &str) {
    // Do nothing for unsupported OS
}

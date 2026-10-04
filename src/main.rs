use std::fs::OpenOptions;
use std::path::PathBuf;

use cosmic_connect::app::CosmicConnect;

const LOG_MAX_BYTES: u64 = 1024 * 1024;

fn log_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;

    let dir = base.join("cosmic-connect");
    std::fs::create_dir_all(&dir).ok()?;

    let path = dir.join("cosmic-connect.log");
    if std::fs::metadata(&path).map(|m| m.len() > LOG_MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("cosmic-connect.log.1"));
    }

    Some(path)
}

fn main() -> cosmic::iced::Result {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));

    // cosmic-panel starts applets with stdout/stderr as pipes, and journalctl showed
    // no output from this process - so stderr logs appear to be discarded.
    // Write to a file instead.
    match log_file().map(|p| OpenOptions::new().create(true).append(true).open(&p)) {
        Some(Ok(file)) => {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
        _ => eprintln!("cosmic-connect: no log file available, falling back to stderr"),
    }

    builder.init();

    cosmic::applet::run::<CosmicConnect>(())?;

    Ok(())
}

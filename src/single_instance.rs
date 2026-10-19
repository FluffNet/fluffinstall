//! Keeps one installer window per user and forwards later launches to it.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LISTENER: OnceLock<Mutex<UnixListener>> = OnceLock::new();

pub struct InstanceGuard {
    socket_path: PathBuf,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}

fn socket_path() -> Result<PathBuf, String> {
    let runtime_dir =
        env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())?;
    Ok(PathBuf::from(runtime_dir).join("fluffinstall.sock"))
}

fn notify_existing(path: &PathBuf) -> io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(b"activate\n")
}

fn bind_listener(path: &PathBuf) -> io::Result<UnixListener> {
    let listener = UnixListener::bind(path)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Returns `None` after notifying an already-running installer.
pub fn claim_or_notify() -> Result<Option<InstanceGuard>, String> {
    let path = socket_path()?;

    let listener = match bind_listener(&path) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            if notify_existing(&path).is_ok() {
                return Ok(None);
            }

            // The previous process may have crashed before removing its socket.
            fs::remove_file(&path)
                .map_err(|error| format!("could not remove stale instance socket: {error}"))?;
            bind_listener(&path)
                .map_err(|error| format!("could not create instance socket: {error}"))?
        }
        Err(error) => return Err(format!("could not create instance socket: {error}")),
    };

    LISTENER
        .set(Mutex::new(listener))
        .map_err(|_| "instance listener was already initialized".to_string())?;

    Ok(Some(InstanceGuard { socket_path: path }))
}

pub fn take_activation_request() -> bool {
    let Some(listener) = LISTENER.get() else {
        return false;
    };
    let Ok(listener) = listener.lock() else {
        return false;
    };

    match listener.accept() {
        Ok((_stream, _address)) => true,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
        Err(error) => {
            eprintln!("Could not receive FluffInstall activation request: {error}");
            false
        }
    }
}

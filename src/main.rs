//! Starts the Qt application and loads the installer interface.

mod backend;
mod installer;
mod single_instance;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QUrl};
use std::process::{Child, Command, Stdio};

unsafe extern "C" {
    fn set_fluffinstall_window_icon();
}

struct SleepInhibitor {
    _process: Child,
}

fn inhibit_sleep() -> Option<SleepInhibitor> {
    match Command::new("/usr/bin/systemd-inhibit")
        .args([
            "--what=sleep",
            "--who=fluffinstall",
            "--why=FluffInstall is open",
            "--mode=block",
            "/usr/bin/cat",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(process) => Some(SleepInhibitor { _process: process }),
        Err(error) => {
            eprintln!("Could not prevent sleep while FluffInstall is open: {error}");
            None
        }
    }
}

fn main() {
    let _instance_guard = match single_instance::claim_or_notify() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(error) => {
            eprintln!("Could not enable single-instance mode: {error}");
            return;
        }
    };

    // The child command reads from a pipe owned by this process. Closing the
    // application closes the pipe and releases the systemd sleep inhibitor.
    let _sleep_inhibitor = inhibit_sleep();
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // KWin uses this name to match the window with fluffinstall.desktop and
    // show the Fluff Linux icon in the task manager.
    QGuiApplication::set_desktop_file_name(&"fluffinstall".into());
    unsafe {
        set_fluffinstall_window_icon();
    }
    QQuickStyle::set_style(&"Fusion".into());

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/org/flufflinux/installer/qml/Main.qml",
        ));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}

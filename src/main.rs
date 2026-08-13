//! Starts the Qt application and loads the installer interface.

mod backend;
mod installer;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QUrl};

unsafe extern "C" {
    fn set_fluffinstall_window_icon();
}

fn main() {
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

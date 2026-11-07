use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::path::Path;

fn main() {
    let has_kf6_i18n = Path::new("/usr/include/KF6/KI18n/KLocalizedContext").exists();
    unsafe {
        CxxQtBuilder::new_qml_module(
            QmlModule::new("org.flufflinux.installer")
                .qml_file("qml/Main.qml")
                .qml_file("qml/DiskIcon.qml"),
        )
        .qrc_resources([
            "assets/usb-storage.svg",
            "assets/drive-harddisk.svg",
            "assets/dialog-error.svg",
        ])
        .cc_builder(|compiler| {
            // GCC 16 can emit this warning while parsing Qt 6 QChar headers.
            // All other compiler warnings remain visible.
            compiler.flag_if_supported("-Wno-sfinae-incomplete");
            if has_kf6_i18n {
                compiler.include("/usr/include/KF6");
                compiler.include("/usr/include/KF6/KI18n");
            }
        })
        .files(["src/backend.rs"])
        .cpp_file("src/window_icon.cpp")
        .build();
    }

    if has_kf6_i18n {
        println!("cargo:rustc-link-lib=KF6I18n");
    }
}

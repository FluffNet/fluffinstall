use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    unsafe {
        CxxQtBuilder::new_qml_module(
            QmlModule::new("org.flufflinux.installer")
                .qml_file("qml/Main.qml")
                .qml_file("qml/DiskIcon.qml"),
        )
        .qrc_resources(["assets/usb-storage.svg"])
        .cc_builder(|compiler| {
            // GCC 16 can emit this warning while parsing Qt 6 QChar headers.
            // All other compiler warnings remain visible.
            compiler.flag_if_supported("-Wno-sfinae-incomplete");
        })
        .files(["src/backend.rs"])
        .cpp_file("src/window_icon.cpp")
        .build();
    }
}

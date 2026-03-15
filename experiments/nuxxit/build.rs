use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=qml/main.qml");
    println!("cargo:rerun-if-changed=src/level_object.rs");

    CxxQtBuilder::new_qml_module(
        QmlModule::new("org.usit.nuxxit").qml_file("qml/main.qml"),
    )
    .qt_module("Quick")
    .qt_module("Network")
    .files(["src/level_object.rs"])
    .build();
}

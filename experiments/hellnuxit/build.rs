use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=cpp/bridge.cpp");
    println!("cargo:rerun-if-changed=cpp/bridge.h");

    let widgets = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("Qt6Widgets")
        .expect("Qt6Widgets pkg-config probe failed");
    let gui = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("Qt6Gui")
        .expect("Qt6Gui pkg-config probe failed");
    let core = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("Qt6Core")
        .expect("Qt6Core pkg-config probe failed");

    let mut build = cc::Build::new();
    build.cpp(true);
    build.file("cpp/bridge.cpp");
    build.flag_if_supported("-std=c++17");

    for path in widgets
        .include_paths
        .iter()
        .chain(gui.include_paths.iter())
        .chain(core.include_paths.iter())
    {
        build.include(path);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    build.out_dir(&out_dir);
    build.compile("hellnuxit_qt");
}

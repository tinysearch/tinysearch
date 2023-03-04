extern crate includedir_codegen;

use includedir_codegen::Compression;

fn main() {
    includedir_codegen::start("FILES")
        .dir(format!("{}{}",std::env::var("OUT_DIR").unwrap(),"/engine"), Compression::Gzip)
        .dir(format!("{}{}",std::env::var("OUT_DIR").unwrap(),"/shared"), Compression::Gzip)
        .build("engine.rs")
        .unwrap();
}

extern crate includedir_codegen;

use includedir_codegen::Compression;

fn main() {
    includedir_codegen::start("FILES")
        .dir(concat!(env!("OUT_DIR"),"/engine"), Compression::Gzip)
        .dir(concat!(env!("OUT_DIR"),"/shared"), Compression::Gzip)
        .build("engine.rs")
        .unwrap();
}

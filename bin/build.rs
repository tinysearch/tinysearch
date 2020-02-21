extern crate includedir_codegen;

use includedir_codegen::Compression;

fn main() {
    includedir_codegen::start("FILES")
        .dir("../engine", Compression::Gzip)
        .dir("../shared", Compression::Gzip)
        .build("engine.rs")
        .unwrap();
}

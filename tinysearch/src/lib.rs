#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

mod index;
mod storage;
mod strip_markdown;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};

use failure::{Error, ResultExt};
use fs::File;
use index::Posts;
use lazy_static::lazy_static;
use tempdir::TempDir;

include!(concat!(env!("OUT_DIR"), "/engine.rs"));

lazy_static! {
    static ref DEMO_HTML: &'static [u8] = include_bytes!("../assets/demo.html");
}

pub fn generate(index: PathBuf, out_path: PathBuf, optim: bool) -> Result<(), Error> {
    let posts: Posts = index::read(fs::read_to_string(index)?)?;
    trace!("{:#?}", posts);
    storage::gen(posts)?;

    let temp_dir = TempDir::new("wasm")?;
    println!("Extracting tinysearch WASM engine");
    extract_engine(&temp_dir.path())?;
    debug!("Crate content extracted to {:?}/", &temp_dir);

    println!("Copying index into crate");
    fs::copy("storage", temp_dir.path().join("engine/storage"))?;

    println!("Compiling WASM module using wasm-pack");
    wasm_pack(&temp_dir.path().join("engine"), &out_path)?;

    if optim {
        optimize(&out_path)?;
    }

    fs::write("demo.html", String::from_utf8_lossy(&DEMO_HTML).to_string())?;

    println!("All done. Open the output folder with a web server to try a demo.");
    Ok(())
}

fn extract_engine(temp_dir: &Path) -> Result<(), Error> {
    for file in FILES.file_names() {
        // This hack removes the "../" prefix that
        // gets introduced by including the crates
        // from the `bin` parent directory.
        let filepath = file.trim_start_matches("../");
        let outpath = temp_dir.join(filepath);
        if let Some(parent) = outpath.parent() {
            debug!("Creating parent dir {:?}", &parent);
            fs::create_dir_all(&parent)?;
        }
        debug!("Extracting {:?}", &outpath);
        let content = FILES.get(file)?;
        let mut outfile = File::create(&outpath)?;
        outfile.write_all(&content)?;
    }
    Ok(())
}

fn wasm_pack(in_dir: &Path, out_dir: &PathBuf) -> Result<String, Error> {
    Ok(run_output(
        Command::new("wasm-pack")
            .arg("build")
            .arg(in_dir)
            .arg("--target")
            .arg("web")
            .arg("--release")
            .arg("--out-dir")
            .arg(out_dir),
    )?)
}

fn optimize(dir: &PathBuf) -> Result<String, Error> {
    Ok(run_output(
        Command::new("wasm-opt")
            .current_dir(dir)
            .arg("-Oz")
            .arg("-o")
            .arg("tinysearch_engine_bg.wasm")
            .arg("tinysearch_engine_bg.wasm"),
    )?)
}

pub fn run_output(cmd: &mut Command) -> Result<String, Error> {
    log::debug!("running {:?}", cmd);
    let output = cmd
        .stderr(Stdio::inherit())
        .output()
        .context(format!("failed to run {:?}", cmd))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        failure::bail!("failed to execute {:?}\nstatus: {}", cmd, output.status)
    }
}

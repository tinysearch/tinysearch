#[macro_use]
extern crate log;

mod index;
mod storage;

use anyhow::{bail, Context, Error, Result};
use argh::FromArgs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};
use tempfile::tempdir;

use fs::File;
use index::Posts;

// The search engine code gets statically included into the binary.
// During indexation (when running tinysearch), this will be compiled to WASM.
include!(concat!(env!("OUT_DIR"), "/engine.rs"));

// Include a bare-bones HTML page that demonstrates how tinysearch is used
static DEMO_HTML: &str = include_str!("../assets/demo.html");

#[derive(FromArgs)]
/// A tiny, static search engine for static websites
struct Opt {
    /// show version and exit
    #[argh(switch)]
    version: bool,

    /// index JSON file to process
    #[argh(positional)]
    index: Option<PathBuf>,

    /// output path for WASM module (local directory by default)
    #[argh(option, short = 'p', long = "path")]
    out_path: Option<PathBuf>,

    /// optimize the output using binaryen
    #[argh(switch, short = 'o', long = "optimize")]
    optimize: bool,
}

fn unpack_engine(temp_dir: &Path) -> Result<(), Error> {
    println!("Starting unpack");
    for file in FILES.file_names() {
        println!("Copying {:?}", file);
        // This hack removes the "../" prefix that
        // gets introduced by including the crates
        // from the `bin` parent directory.
        let filepath = file.trim_start_matches("../");
        let outpath = temp_dir.join(filepath);
        if let Some(parent) = outpath.parent() {
            debug!("Creating parent dir {:?}", &parent);
            fs::create_dir_all(&parent)?;
        }
        let content = FILES.get(file)?;
        let mut outfile = File::create(&outpath)?;
        outfile.write_all(&content)?;
    }
    Ok(())
}

fn main() -> Result<(), Error> {
    FILES.set_passthrough(env::var_os("PASSTHROUGH").is_some());

    let opt: Opt = argh::from_env();

    if opt.version {
        println!("tinysearch {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    let out_path = opt
        .out_path
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()?;

    let index = opt.index.context("No index file specified")?;
    let posts: Posts = index::read(fs::read_to_string(index)?)?;
    trace!("Generating storage from posts: {:#?}", posts);
    storage::write(posts)?;

    let temp_dir = tempdir()?;
    println!(
        "Unpacking tinysearch WASM engine into temporary directory {:?}",
        temp_dir.path()
    );
    unpack_engine(temp_dir.path())?;
    debug!("Crate content extracted to {:?}/", &temp_dir);

    let engine_dir = temp_dir.path().join("engine");
    if !engine_dir.exists() {
        for path in fs::read_dir(out_path)? {
            println!("Name: {}", path.unwrap().path().display())
        }
        bail!(
            "Engine directory could not be created at {}",
            engine_dir.display()
        );
    }

    println!("Copying index into crate");
    fs::rename("storage", engine_dir.join("storage"))?;

    println!("Compiling WASM module using wasm-pack");
    wasm_pack(&temp_dir.path().join("engine"), &out_path)?;

    if opt.optimize {
        optimize(&out_path)?;
    }

    fs::write(&out_path.join("demo.html"), DEMO_HTML)?;

    println!("All done! Open the output folder with a web server to try the demo.");
    Ok(())
}

fn wasm_pack(in_dir: &Path, out_dir: &Path) -> Result<String, Error> {
    run_output(
        Command::new("wasm-pack")
            .arg("build")
            .arg(in_dir)
            .arg("--target")
            .arg("web")
            .arg("--release")
            .arg("--out-dir")
            .arg(out_dir),
    )
}

fn optimize(dir: &Path) -> Result<String, Error> {
    run_output(
        Command::new("wasm-opt")
            .current_dir(dir)
            .arg("-Oz")
            .arg("-o")
            .arg("tinysearch_engine_bg.wasm")
            .arg("tinysearch_engine_bg.wasm"),
    )
}

pub fn run_output(cmd: &mut Command) -> Result<String, Error> {
    log::debug!("running {:?}", cmd);
    let output = cmd
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to run {:?}", cmd))?;

    if !output.status.success() {
        anyhow::bail!("failed to execute {:?}\nstatus: {}", cmd, output.status)
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

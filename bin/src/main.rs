#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate structopt_derive;
#[macro_use]
extern crate log;

use failure::{Error, ResultExt};
use tempdir::TempDir;

mod download;
mod index;
mod storage;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use structopt::StructOpt;

use download::download_engine;
use index::Posts;

lazy_static! {
    static ref DEMO_HTML: &'static [u8] = include_bytes!("../demo.html");
}

#[derive(StructOpt, Debug)]
struct Opt {
    /// index JSON file to process
    #[structopt(name = "index", parse(from_os_str))]
    index: PathBuf,

    /// Output path for WASM module
    #[structopt(short = "p", long = "path", parse(from_os_str))]
    out_path: PathBuf,

    /// Optimize the output using binaryen
    #[structopt(short = "o", long = "optimize")]
    optimize: bool,
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();

    let posts: Posts = index::read(fs::read_to_string(opt.index)?)?;
    println!("{:#?}", posts);
    storage::gen(posts)?;

    let temp_dir = TempDir::new("wasm")?;
    let download_dir = download_engine(&temp_dir.path())?;
    println!("Crate content extracted to {}/", download_dir.display());

    println!("Copying index into crate");
    fs::copy("storage", &download_dir.join("storage"))?;

    println!("Compiling WASM module using wasm-pack");
    wasm_pack(&download_dir, &opt.out_path)?;

    if opt.optimize {
        optimize(&opt.out_path)?;
    }

    fs::write("demo.html", String::from_utf8_lossy(&DEMO_HTML).to_string())?;

    println!("All done. Open the output folder with a webserver to try a demo.");
    Ok(())
}

fn optimize(dir: &PathBuf) -> Result<String, Error> {
    Ok(run_output(
        Command::new("wasm-opt")
            .arg("-Oz")
            .arg("-o")
            .arg("tinysearch_engine_bg.wasm")
            .arg("tinysearch_engine_bg.wasm")
            .current_dir(dir),
    )?)
}

fn wasm_pack(in_dir: &PathBuf, out_dir: &PathBuf) -> Result<String, Error> {
    Ok(run_output(
        Command::new("wasm-pack")
            .arg("build")
            .arg("--target")
            .arg("web")
            .arg("--release")
            .arg("--out-dir")
            .arg(out_dir)
            .current_dir(in_dir),
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

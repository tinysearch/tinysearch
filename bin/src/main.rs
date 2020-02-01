#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

extern crate structopt_derive;

#[macro_use]
extern crate log;

use failure::{Error, ResultExt};
use tempdir::TempDir;

mod download;
mod index;
mod storage;
mod strip_markdown;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs};
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
    #[structopt(short = "p", long = "path", parse(from_os_str), default_value = ".")]
    out_path: PathBuf,

    /// Optimize the output using binaryen
    #[structopt(short = "o", long = "optimize")]
    optimize: bool,
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    let out_path = opt.out_path.canonicalize()?;

    let posts: Posts = index::read(fs::read_to_string(opt.index)?)?;
    trace!("{:#?}", posts);
    storage::gen(posts)?;

    let temp_dir = TempDir::new("wasm")?;
    println!("Downloading tinysearch WASM library");
    let download_dir = download_engine(&temp_dir.path())?;
    debug!("Crate content extracted to {}/", download_dir.display());

    println!("Copying index into crate");
    fs::copy("storage", &download_dir.join("storage"))?;

    println!("Compiling WASM module using wasm-pack");
    env::set_var(
        "CARGO_TARGET_DIR",
        env::current_dir()?.join("tinysearch_build"),
    );
    wasm_pack(&download_dir, &out_path)?;

    if opt.optimize {
        optimize(&out_path)?;
    }

    fs::write("demo.html", String::from_utf8_lossy(&DEMO_HTML).to_string())?;

    println!("All done. Open the output folder with a web server to try a demo.");
    Ok(())
}

fn wasm_pack(in_dir: &PathBuf, out_dir: &PathBuf) -> Result<String, Error> {
    Ok(run_output(
        Command::new("wasm-pack")
            .current_dir(in_dir)
            .arg("build")
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

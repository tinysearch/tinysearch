#![cfg(feature = "bin")]
#[macro_use]
extern crate log;

mod utils;
use utils::assets;
use utils::index;
use utils::storage;

use anyhow::{bail, Context};
pub use anyhow::{Error, Result};
use argh::FromArgs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::{env, fs};
use tempfile::TempDir;
use toml_edit::{value, Document};

use index::Posts;
use strum::{EnumString, IntoStaticStr};

fn ensure_exists(path: PathBuf) -> Result<PathBuf, Error> {
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    let path = path.canonicalize()?;
    if !path.exists() {
        for path in fs::read_dir(&path)? {
            println!("Name: {}", path.unwrap().path().display())
        }
        bail!("Directory could not be created at {}", &path.display());
    }
    Ok(path)
}

#[derive(Debug)]
enum DirOrTemp {
    Path(PathBuf),
    Temp(TempDir),
}

impl DirOrTemp {
    pub fn path(&self) -> PathBuf {
        match self {
            DirOrTemp::Path(p) => p.clone(),
            DirOrTemp::Temp(p) => p.path().to_path_buf(),
        }
    }
}

impl Default for DirOrTemp {
    fn default() -> Self {
        Self::Temp(TempDir::new().expect("Failed to create a temporary directory"))
    }
}

impl FromStr for DirOrTemp {
    type Err = <PathBuf as FromStr>::Err;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(DirOrTemp::Path(PathBuf::from_str(s)?))
    }
}

#[derive(IntoStaticStr, EnumString, Clone)]
#[strum(serialize_all = "snake_case")]
enum OutputMode {
    Search,
    Storage,
    Crate,
    Wasm,
}

fn parse_engine_version(str: &str) -> Result<toml_edit::Table, String> {
    let doc = str.parse::<Document>().map_err(|e| e.to_string())?;
    Ok(doc.as_table().clone())
}

#[derive(FromArgs, Clone)]
/// A tiny, static search engine for static websites
///
///
/// It can run in several modes (-m/--mode argument).
/// Valid modes are:
/// **search** - runs search engine on generated storage data,
/// **storage** - generates storage data for posts,
/// **crate** - creates a Rust crate with storage data,
/// **wasm** - creates a crate and generates a loadable js/wasm script.
///
struct Opt {
    /// show version and exit
    #[argh(switch)]
    version: bool,

    /// output mode
    #[argh(option, short = 'm', long = "mode", default = "OutputMode::Wasm")]
    output_mode: OutputMode,

    /// term to search in posts (only for search mode)
    #[argh(
        option,
        short = 'S',
        long = "search-term",
        default = "String::default()"
    )]
    search_term: String,

    /// number of posts to show in search results (only for search mode)
    #[argh(option, short = 'N', long = "num-searches", default = "5")]
    num_searches: usize,

    /// input file to process (either JSON with posts for code generation or storage for inference)
    #[argh(positional)]
    input_file: Option<PathBuf>,

    /// output path for WASM module ("wasm_output" directory by default)
    #[argh(
        option,
        short = 'p',
        long = "path",
        default = "\"./wasm_output\".into()"
    )]
    out_path: PathBuf,

    /// where to put generated crate
    /// * In wasm mode crate is generated:
    ///   * If this option is specified: in this path.
    ///   * If this option is omitted: in a temp directory removed after run.
    /// * In crate mode this is ignored in favor of -p/--path.
    #[argh(option, long = "crate-path")]
    crate_path: Option<PathBuf>,

    /// this version will be used in Cargo.toml for the generated crate
    /// (only used in wasm, crate modes). This should be a valid TOML table definition.
    /// Default is 'version="env!("CARGO_PKG_VERSION")"'. If you have a local version of
    /// tinysearch, you can specify 'path="/path/to/tinysearch"'
    #[argh(
        option,
        short = 'e',
        long = "engine-version",
        from_str_fn(parse_engine_version),
        default = "format!(\"version=\\\"{}\\\"\", env!(\"CARGO_PKG_VERSION\")).parse::<toml_edit::Document>().unwrap().as_table().clone()"
    )]
    engine_version: toml_edit::Table,

    /// this name will be used in Cargo.toml for the generated crate (only used in wasm and crate modes)
    #[argh(option, long = "crate-name", default = "\"tinysearch-engine\".into()")]
    crate_name: String,

    /// removes all top-level configs from Cargo.toml of generated crate and makes it locally importable (only makes sense in crate mode)
    #[argh(switch, long = "non-top-level-crate")]
    non_top_level_crate: bool,

    /// optimize the output using binaryen (only valid in wasm mode)
    #[argh(switch, short = 'o', long = "optimize")]
    optimize: bool,
}

trait Stage: Sized {
    fn from_opt(opt: &Opt) -> Result<Self, Error>;

    fn build(&self) -> Result<(), Error>;
}

#[derive(Default)]
struct Search {
    storage_file: PathBuf,
    term: String,
    num_searches: usize,
}

impl Stage for Search {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        let input = opt.input_file.clone().context("Missing input file")?;
        let term = opt.search_term.clone();
        Ok(Self {
            storage_file: input
                .canonicalize()
                .with_context(|| format!("Failed to find file: {}", input.display()))?,
            term,
            num_searches: opt.num_searches,
        })
    }

    fn build(&self) -> Result<(), Error> {
        use tinysearch::{search as base_search, Storage};
        let bytes = fs::read(&self.storage_file).with_context(|| {
            format!("Failed to read input file: {}", self.storage_file.display())
        })?;
        let filters = Storage::from_bytes(&bytes)?.filters;
        let results = base_search(&filters, self.term.clone(), self.num_searches);
        for result in results {
            println!(
                "Title: {}, Url: {}, Meta: {:?}",
                result.0, result.1, result.2
            );
        }
        Ok(())
    }
}

#[derive(Default)]
struct Storage {
    posts_index: PathBuf,
    out_path: PathBuf,
}

impl Stage for Storage {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        Ok(Self {
            posts_index: opt.input_file.clone().context("No input file")?,
            out_path: ensure_exists(opt.out_path.clone())?,
        })
    }

    fn build(&self) -> Result<(), Error> {
        let storage_file = self.out_path.join("storage");
        println!(
            "Creating storage file for posts {} in file {}",
            self.posts_index.display(),
            storage_file.display()
        );
        let posts: Posts = index::read(
            fs::read_to_string(&self.posts_index)
                .with_context(|| format!("Failed to read file {}", self.posts_index.display()))?,
        )
        .with_context(|| format!("Failed to decode {}", self.posts_index.display()))?;
        trace!("Generating storage from posts: {:#?}", posts);
        storage::write(posts, &storage_file)?;
        println!("Storage ready in file {}", storage_file.display());
        Ok(())
    }
}

#[derive(Default)]
struct Crate {
    s: Storage,
    out_path: PathBuf,
    crate_name: String,
    engine_version: toml_edit::Table,
    non_top_level: bool,
}

impl Stage for Crate {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        if opt.crate_path.is_some() {
            bail!("Don't use --crate-path to specify crate output dir!");
        }
        let out_path = ensure_exists(opt.out_path.clone())?;
        let storage_opt = {
            let mut ret: Opt = opt.clone();
            ret.out_path = ensure_exists(out_path.join("src"))?;
            ret
        };

        Ok(Self {
            s: Storage::from_opt(&storage_opt)?,
            out_path,
            crate_name: opt.crate_name.clone(),
            engine_version: opt.engine_version.clone(),
            non_top_level: opt.non_top_level_crate,
        })
    }

    fn build(&self) -> Result<(), Error> {
        println!(
            "Creating tinysearch implementation crate {} in directory {}",
            self.crate_name,
            self.out_path.display()
        );
        let cargo_toml = self.out_path.join("Cargo.toml");
        let mut cargo_toml_contents = assets::CRATE_CARGO_TOML.parse::<Document>()?;
        cargo_toml_contents["package"]["name"] = value(self.crate_name.clone());
        cargo_toml_contents["dependencies"]["tinysearch"] =
            toml_edit::Item::Table(self.engine_version.clone());
        if self.non_top_level {
            cargo_toml_contents.as_table_mut().remove("workspace");
            cargo_toml_contents.as_table_mut().remove("profile");
            cargo_toml_contents.as_table_mut().remove("lib");
            cargo_toml_contents["lib"] = toml_edit::table();
        }
        fs::write(cargo_toml, cargo_toml_contents.to_string())?;

        // let mut file = fs::OpenOptions::new().write(true).truncate(true).open(&cargo_toml)?;
        // file.write(new.as_bytes())?;

        self.s.build().context("Failed building storage")?;
        fs::write(
            self.out_path.join("src").join("lib.rs"),
            assets::CRATE_LIB_RS,
        )?;
        println!("Crate content generated in {}/", &self.out_path.display());
        Ok(())
    }
}

#[derive(Default)]
struct Wasm {
    c: Crate,
    out_path: PathBuf,
    crate_path: DirOrTemp,
    optimize: bool,
}

impl Wasm {
    fn ensure_crate_path(crate_path: &Option<PathBuf>) -> Result<DirOrTemp, Error> {
        Ok(match crate_path {
            Some(p) => DirOrTemp::Path(ensure_exists(p.clone())?),
            None => DirOrTemp::default(),
        })
    }
}

impl Stage for Wasm {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        let crate_path = Wasm::ensure_crate_path(&opt.crate_path)?;
        let crate_opt = {
            let mut ret: Opt = opt.clone();
            ret.out_path = crate_path.path();
            ret.crate_path = None;
            ret
        };
        Ok(Self {
            c: Crate::from_opt(&crate_opt)?,
            out_path: ensure_exists(opt.out_path.clone())?,
            crate_path,
            optimize: opt.optimize,
        })
    }

    fn build(self: &Wasm) -> Result<(), Error> {
        self.c.build().context("Failed generating crate")?;
        println!("Compiling WASM module using wasm-pack");
        let crate_path = self.crate_path.path();
        run_output(
            Command::new("wasm-pack")
                .arg("build")
                .arg(&crate_path)
                .arg("--target")
                .arg("web")
                .arg("--release")
                .arg("--out-dir")
                .arg(&self.out_path),
        )?;
        let wasm_name = self.c.crate_name.replace('-', "_");

        if self.optimize {
            let wasm_file = format!("{}_bg.wasm", &wasm_name);
            run_output(
                Command::new("wasm-opt")
                    .current_dir(&self.out_path)
                    .arg("-Oz")
                    .arg("-o")
                    .arg(&wasm_file)
                    .arg(&wasm_file),
            )?;
        }
        let html_path = self.out_path.join("demo.html");
        fs::write(
            &html_path,
            assets::DEMO_HTML.replace("{WASM_NAME}", &wasm_name),
        )
        .with_context(|| format!("Failed writing demo.html to {}", &html_path.display()))?;
        println!("All done! Open the output folder with a web server to try the demo.");
        Ok(())
    }
}

pub fn main() -> Result<(), Error> {
    let opt: Opt = argh::from_env();

    if opt.version {
        println!("tinysearch {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    let parse_ctx = || {
        format!(
            "Failed to parse options for {} mode",
            Into::<&'static str>::into(&opt.output_mode)
        )
    };

    match opt.output_mode {
        OutputMode::Search => Search::from_opt(&opt).with_context(parse_ctx)?.build(),
        OutputMode::Storage => Storage::from_opt(&opt).with_context(parse_ctx)?.build(),
        OutputMode::Crate => Crate::from_opt(&opt).with_context(parse_ctx)?.build(),
        OutputMode::Wasm => Wasm::from_opt(&opt).with_context(parse_ctx)?.build(),
    }
    .with_context(|| {
        format!(
            "Failed to build {} mode",
            Into::<&'static str>::into(&opt.output_mode)
        )
    })
}

pub fn run_output(cmd: &mut Command) -> Result<String, Error> {
    println!("running {:?}", cmd);
    let output = cmd
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to run {:?}", cmd))?;

    if !output.status.success() {
        anyhow::bail!("failed to execute {:?}\nstatus: {}", cmd, output.status)
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_compile_example(){
//         run_output(
//             Command::new("/home/delphi/.cargo/bin/trunk")
//             .current_dir("../examples/yew-example-storage")
//             .arg("build")
//             .arg("--release")
//         ).unwrap();
//     }
// }

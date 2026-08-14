//! Command-line interface for building and querying tinysearch indexes.

#![cfg(feature = "bin")]
#[macro_use]
extern crate log;

mod utils;
use utils::assets;
use utils::index;
use utils::storage;

use anyhow::{Context, bail};
use anyhow::{Error, Result};
use argh::FromArgs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::{env, fs};
use tempfile::TempDir;
use tinysearch::{IndexKind, SearchSchema};
use toml_edit::{DocumentMut, value};

use index::Posts;
use strum::{EnumString, IntoStaticStr};

fn ensure_exists(path: &Path) -> Result<PathBuf, Error> {
    fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory at {}", path.display()))?;
    path.canonicalize()
        .with_context(|| format!("Failed to resolve directory at {}", path.display()))
}

#[derive(Debug)]
enum DirOrTemp {
    Path(PathBuf),
    Temp(TempDir),
}

impl DirOrTemp {
    fn path(&self) -> PathBuf {
        match self {
            Self::Path(p) => p.clone(),
            Self::Temp(p) => p.path().to_path_buf(),
        }
    }
}

impl FromStr for DirOrTemp {
    type Err = <PathBuf as FromStr>::Err;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self::Path(PathBuf::from_str(s)?))
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

fn parse_engine_version(input: &str) -> Result<toml_edit::Table, String> {
    let doc = input
        .parse::<DocumentMut>()
        .map_err(|error| error.to_string())?;
    Ok(doc.as_table().clone())
}

fn default_engine_version() -> toml_edit::Table {
    let mut dependency = toml_edit::Table::new();
    dependency.insert("version", value(env!("CARGO_PKG_VERSION")));
    dependency
}

#[derive(FromArgs, Clone)]
#[allow(clippy::struct_excessive_bools)]
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

    /// create production-ready output without demo files
    #[argh(switch)]
    release: bool,

    /// output mode
    #[argh(option, short = 'm', long = "mode", default = "OutputMode::Wasm")]
    output_mode: OutputMode,

    /// index backend used when creating storage (exact or xor8)
    #[argh(option, long = "indexer", default = "IndexKind::Exact")]
    index_kind: IndexKind,

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

    /// output path for WASM module ("`wasm_output`" directory by default)
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
    /// Default is '`version="env!("CARGO_PKG_VERSION`")"'. If you have a local version of
    /// tinysearch, you can specify 'path="/path/to/tinysearch"'
    #[argh(
        option,
        short = 'e',
        long = "engine-version",
        from_str_fn(parse_engine_version),
        default = "default_engine_version()"
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
        use tinysearch::{Storage, search as base_search};
        let bytes = fs::read(&self.storage_file).with_context(|| {
            format!("Failed to read input file: {}", self.storage_file.display())
        })?;
        let filters = Storage::from_bytes(&bytes)?.filters;
        let results = base_search(&filters, &self.term, self.num_searches);
        for result in &results {
            println!(
                "Title: {title}, Url: {url}, Meta: {meta}",
                title = result.title,
                url = result.url,
                meta = result.meta
            );
        }
        Ok(())
    }
}

struct Storage {
    posts_index: PathBuf,
    out_path: PathBuf,
    schema: SearchSchema,
    index_kind: IndexKind,
}

impl Stage for Storage {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        let posts_index = opt.input_file.clone().context("No input file")?;
        let parent_dir = posts_index
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let schema = SearchSchema::load_from_file(parent_dir)
            .map_err(|error| anyhow::anyhow!("Failed to load schema: {error}"))?;

        Ok(Self {
            posts_index,
            out_path: ensure_exists(&opt.out_path)?,
            schema,
            index_kind: opt.index_kind,
        })
    }

    fn build(&self) -> Result<(), Error> {
        let storage_file = self.out_path.join("storage");
        println!(
            "Creating storage file for posts {} in file {}",
            self.posts_index.display(),
            storage_file.display()
        );

        let raw_content = fs::read_to_string(&self.posts_index)
            .with_context(|| format!("Failed to read file {}", self.posts_index.display()))?;

        let posts: Posts = index::read(&raw_content)
            .with_context(|| format!("Failed to decode {}", self.posts_index.display()))?;
        trace!("Generating storage from posts: {posts:#?}");
        storage::write(posts, &storage_file, &self.schema, self.index_kind)?;

        println!("Storage ready in file {}", storage_file.display());
        Ok(())
    }
}

struct Crate {
    s: Storage,
    out_path: PathBuf,
    name: String,
    engine_version: toml_edit::Table,
    non_top_level: bool,
}

impl Stage for Crate {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        if opt.crate_path.is_some() {
            bail!("Don't use --crate-path to specify crate output dir!");
        }
        let out_path = ensure_exists(&opt.out_path)?;
        let storage_opt = {
            let mut ret: Opt = opt.clone();
            ret.out_path = ensure_exists(&out_path.join("src"))?;
            ret
        };

        Ok(Self {
            s: Storage::from_opt(&storage_opt)?,
            out_path,
            name: opt.crate_name.clone(),
            engine_version: opt.engine_version.clone(),
            non_top_level: opt.non_top_level_crate,
        })
    }

    fn build(&self) -> Result<(), Error> {
        println!(
            "Creating tinysearch implementation crate {} in directory {}",
            self.name,
            self.out_path.display()
        );
        let cargo_toml = self.out_path.join("Cargo.toml");
        let mut cargo_toml_contents = assets::CRATE_CARGO_TOML.parse::<DocumentMut>()?;
        cargo_toml_contents["package"]["name"] = value(self.name.clone());
        cargo_toml_contents["dependencies"]["tinysearch"] =
            toml_edit::Item::Table(self.engine_version.clone());
        if self.non_top_level {
            cargo_toml_contents.as_table_mut().remove("workspace");
            cargo_toml_contents.as_table_mut().remove("profile");
            cargo_toml_contents.as_table_mut().remove("lib");
            cargo_toml_contents["lib"] = toml_edit::table();
        }
        fs::write(cargo_toml, cargo_toml_contents.to_string())?;

        self.s.build().context("Failed building storage")?;
        fs::write(
            self.out_path.join("src").join("lib.rs"),
            assets::CRATE_LIB_RS,
        )?;
        println!("Crate content generated in {}/", self.out_path.display());
        Ok(())
    }
}

struct Wasm {
    c: Crate,
    out_path: PathBuf,
    crate_path: DirOrTemp,
    optimize: bool,
    release: bool,
}

impl Wasm {
    fn ensure_crate_path(crate_path: Option<&Path>) -> Result<DirOrTemp, Error> {
        match crate_path {
            Some(path) => Ok(DirOrTemp::Path(ensure_exists(path)?)),
            None => TempDir::new()
                .map(DirOrTemp::Temp)
                .context("Failed to create a temporary directory"),
        }
    }
}

impl Stage for Wasm {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        let crate_path = Self::ensure_crate_path(opt.crate_path.as_deref())?;
        let crate_opt = {
            let mut ret: Opt = opt.clone();
            ret.out_path = crate_path.path();
            ret.crate_path = None;
            ret
        };
        Ok(Self {
            c: Crate::from_opt(&crate_opt)?,
            out_path: ensure_exists(&opt.out_path)?,
            crate_path,
            optimize: opt.optimize,
            release: opt.release,
        })
    }

    fn build(&self) -> Result<(), Error> {
        self.c.build().context("Failed generating crate")?;
        println!("Compiling WASM module using vanilla cargo build");
        let crate_path = self.crate_path.path();
        let wasm_name = self.c.name.replace('-', "_");

        // Build with vanilla cargo
        run_output(
            Command::new("cargo")
                .current_dir(&crate_path)
                .arg("build")
                .arg("--target")
                .arg("wasm32-unknown-unknown")
                .arg("--release"),
        )?;

        // Copy the WASM file to output directory
        let wasm_file = format!("{wasm_name}.wasm");
        let source_wasm = crate_path
            .join("target/wasm32-unknown-unknown/release")
            .join(&wasm_file);
        let dest_wasm = self.out_path.join(&wasm_file);
        fs::copy(&source_wasm, &dest_wasm).with_context(|| {
            format!(
                "Failed to copy {} to {}",
                source_wasm.display(),
                dest_wasm.display()
            )
        })?;

        // Generate simple JS loader
        let js_content = assets::JS_LOADER.replace("{WASM_FILE}", &wasm_file);

        let js_path = self.out_path.join(format!("{wasm_name}.js"));
        if !self.release {
            fs::write(&js_path, js_content)
                .with_context(|| format!("Failed writing JS loader to {}", js_path.display()))?;
        }

        // Optional optimization
        if self.optimize {
            if run_output(
                Command::new("wasm-opt")
                    .current_dir(&self.out_path)
                    .arg("--enable-bulk-memory")
                    .arg("-Oz")
                    .arg("-o")
                    .arg(&wasm_file)
                    .arg(&wasm_file),
            )
            .is_ok()
            {
                println!("Optimized WASM with wasm-opt");
            } else {
                println!("wasm-opt unavailable or failed, skipping optimization");
            }
        }

        if self.release {
            println!("Created production-ready WASM module");
            println!("See docs for usage instructions");
            println!("Path: {}", dest_wasm.display());
            println!("Size: {} bytes", dest_wasm.metadata()?.len());
        } else {
            let html_path = self.out_path.join("demo.html");
            fs::write(
                &html_path,
                assets::DEMO_HTML.replace("{WASM_NAME}", &wasm_name),
            )
            .with_context(|| format!("Failed writing demo.html to {}", html_path.display()))?;
            println!("All done! WASM module at: {}", dest_wasm.display());
            println!("JS loader at: {}", js_path.display());
            println!("Demo at: {}", html_path.display());
        }
        Ok(())
    }
}

/// Runs tinysearch using command-line arguments.
///
/// # Errors
///
/// Returns an error if the selected mode cannot parse its inputs or complete
/// its build operation.
fn main() -> Result<(), Error> {
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

/// Runs a child process and returns its standard output.
///
/// # Errors
///
/// Returns an error if the process cannot be started or exits unsuccessfully.
fn run_output(cmd: &mut Command) -> Result<String, Error> {
    println!("running {cmd:?}");
    let output = cmd
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to run {cmd:?}"))?;

    if !output.status.success() {
        anyhow::bail!("failed to execute {:?}\nstatus: {}", cmd, output.status)
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

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
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::{env, fs};
use tempfile::{NamedTempFile, TempDir};
use tinysearch::{IndexKind, SearchIndex, SearchSchema, ShardConfig};
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

fn parse_shard_config(input: &str) -> Result<ShardConfig, String> {
    let target_bytes = input.parse::<usize>().map_err(|error| error.to_string())?;
    ShardConfig::new(target_bytes).map_err(|error| error.to_string())
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

    /// raw target size in bytes for exact index shards (default: 65536)
    #[argh(
        option,
        long = "shard-size",
        from_str_fn(parse_shard_config),
        default = "ShardConfig::default()"
    )]
    shard_config: ShardConfig,

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

    fn build(self) -> Result<(), Error>;
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

    fn build(self) -> Result<(), Error> {
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
    index: SearchIndex,
    index_kind: IndexKind,
}

impl Storage {
    fn into_index(self) -> SearchIndex {
        self.index
    }
}

impl Stage for Storage {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        let posts_index = opt.input_file.clone().context("No input file")?;
        let parent_dir = posts_index.parent().unwrap_or_else(|| Path::new("."));
        let schema = SearchSchema::load_from_file(parent_dir)
            .map_err(|error| anyhow::anyhow!("Failed to load schema: {error}"))?;
        let raw_content = fs::read_to_string(&posts_index)
            .with_context(|| format!("Failed to read file {}", posts_index.display()))?;
        let posts: Posts = index::read(&raw_content)
            .with_context(|| format!("Failed to decode {}", posts_index.display()))?;
        trace!("Generating storage from posts: {posts:#?}");
        let search_index = storage::build(posts, &schema, opt.index_kind);

        Ok(Self {
            posts_index,
            out_path: ensure_exists(&opt.out_path)?,
            index: search_index,
            index_kind: opt.index_kind,
        })
    }

    fn build(self) -> Result<(), Error> {
        let storage_file = self.out_path.join("storage");
        println!(
            "Creating storage file for posts {} in file {}",
            self.posts_index.display(),
            storage_file.display()
        );
        storage::write(self.index, &storage_file)?;

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
    shard_config: ShardConfig,
}

impl Crate {
    fn prepare(&self) -> Result<PathBuf, Error> {
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
        ensure_exists(&self.out_path.join("src"))
    }

    fn build_embedded(self) -> Result<(), Error> {
        let src_dir = self.prepare()?;
        let index_dir = self.out_path.join("index");
        if index_dir.try_exists()? {
            fs::remove_dir_all(&index_dir).with_context(|| {
                format!(
                    "Failed removing sharded index directory {}",
                    index_dir.display()
                )
            })?;
        }

        storage::write(self.s.into_index(), &src_dir.join("storage"))
            .context("Failed building embedded storage")?;
        fs::write(src_dir.join("lib.rs"), assets::CRATE_LIB_RS)?;
        println!("Crate content generated in {}/", self.out_path.display());
        Ok(())
    }

    fn build_sharded_exact(self) -> Result<storage::ShardedArtifacts, Error> {
        if self.s.index_kind != IndexKind::Exact {
            bail!("sharded crate generation requires the exact index backend");
        }

        let src_dir = self.prepare()?;
        let storage_file = src_dir.join("storage");
        if storage_file.try_exists()? {
            fs::remove_file(&storage_file).with_context(|| {
                format!(
                    "Failed to remove legacy embedded storage {}",
                    storage_file.display()
                )
            })?;
        }
        fs::write(src_dir.join("lib.rs"), assets::SHARDED_CRATE_LIB_RS)?;
        let artifacts = storage::write_sharded(
            &self.s.into_index(),
            &self.out_path.join("index"),
            self.shard_config,
        )
        .context("Failed building sharded index artifacts")?;
        println!("Crate content generated in {}/", self.out_path.display());
        Ok(artifacts)
    }

    fn build_for_wasm(self) -> Result<Option<storage::ShardedArtifacts>, Error> {
        match self.s.index_kind {
            IndexKind::Exact => self.build_sharded_exact().map(Some),
            IndexKind::Xor8 => {
                self.build_embedded()?;
                Ok(None)
            }
        }
    }
}

impl Stage for Crate {
    fn from_opt(opt: &Opt) -> Result<Self, Error> {
        if opt.crate_path.is_some() {
            bail!("Don't use --crate-path to specify crate output dir!");
        }
        let out_path = ensure_exists(&opt.out_path)?;

        Ok(Self {
            s: Storage::from_opt(opt)?,
            out_path,
            name: opt.crate_name.clone(),
            engine_version: opt.engine_version.clone(),
            non_top_level: opt.non_top_level_crate,
            shard_config: opt.shard_config,
        })
    }

    fn build(self) -> Result<(), Error> {
        self.build_embedded()
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

    fn build(self) -> Result<(), Error> {
        let crate_path = self.crate_path.path();
        let wasm_name = self.c.name.replace('-', "_");
        let artifacts = self.c.build_for_wasm().context("Failed generating crate")?;
        println!("Compiling WASM module using vanilla cargo build");

        run_command(
            Command::new("cargo")
                .current_dir(&crate_path)
                .arg("build")
                .arg("--target")
                .arg("wasm32-unknown-unknown")
                .arg("--release"),
        )?;

        let wasm_file = format!("{wasm_name}.wasm");
        let source_wasm = crate_path
            .join("target/wasm32-unknown-unknown/release")
            .join(&wasm_file);
        let dest_wasm = self.out_path.join(&wasm_file);

        if let Some(sharded) = &artifacts {
            let source_index = crate_path.join("index");
            for filename in &sharded.shard_files {
                copy_content_addressed_file(
                    &source_index.join(filename),
                    &self.out_path.join(filename),
                )?;
            }
        }

        let wasm_temp = copy_to_temporary(&source_wasm, &self.out_path)?;
        let wasm_temp = if self.optimize {
            let optimized = NamedTempFile::new_in(&self.out_path)
                .context("Failed creating temporary optimized WASM file")?;
            if run_command(
                Command::new("wasm-opt")
                    .current_dir(&self.out_path)
                    .arg("--enable-bulk-memory")
                    .arg("-Oz")
                    .arg("-o")
                    .arg(optimized.path())
                    .arg(wasm_temp.path()),
            )
            .is_ok()
            {
                optimized.as_file().sync_all()?;
                println!("Optimized WASM with wasm-opt");
                optimized
            } else {
                println!("wasm-opt unavailable or failed, skipping optimization");
                wasm_temp
            }
        } else {
            wasm_temp
        };
        let wasm_bytes = fs::metadata(wasm_temp.path())?.len();
        persist_replace(wasm_temp, &dest_wasm)?;

        let loader_template = if artifacts.is_some() {
            assets::JS_LOADER
        } else {
            assets::LEGACY_JS_LOADER
        };
        let js_content = loader_template
            .replace("{WASM_FILE}", &wasm_file)
            .replace("{ROOT_FILE}", storage::ROOT_FILENAME);
        let loader_file = format!("{wasm_name}.js");
        let js_path = self.out_path.join(&loader_file);
        write_atomically(&js_path, js_content.as_bytes())?;

        let html_path = self.out_path.join("demo.html");
        if self.release {
            if html_path.try_exists()? {
                fs::remove_file(&html_path).with_context(|| {
                    format!("Failed removing development demo {}", html_path.display())
                })?;
            }
        } else {
            let demo_html = assets::DEMO_HTML.replace("{LOADER_FILE}", &loader_file);
            write_atomically(&html_path, demo_html.as_bytes())?;
        }

        if let Some(sharded) = &artifacts {
            copy_file_atomically(
                &crate_path.join("index").join(storage::ROOT_FILENAME),
                &self.out_path.join(storage::ROOT_FILENAME),
            )?;
            print_wasm_artifact_sizes(wasm_bytes, sharded);
        } else {
            remove_stale_root(&self.out_path)?;
            println!("Artifact sizes:");
            println!("  WASM: {wasm_bytes} bytes");
        }

        if self.release {
            println!("Created production-ready WASM module");
            println!("WASM module at: {}", dest_wasm.display());
            println!("JS loader at: {}", js_path.display());
        } else {
            println!("All done! WASM module at: {}", dest_wasm.display());
            println!("JS loader at: {}", js_path.display());
            println!("Demo at: {}", html_path.display());
        }
        Ok(())
    }
}

fn copy_to_temporary(source: &Path, directory: &Path) -> Result<NamedTempFile, Error> {
    let mut source_file = fs::File::open(source)
        .with_context(|| format!("Failed opening source artifact {}", source.display()))?;
    let mut temporary = NamedTempFile::new_in(directory).with_context(|| {
        format!(
            "Failed creating temporary artifact in {}",
            directory.display()
        )
    })?;
    io::copy(&mut source_file, temporary.as_file_mut())
        .with_context(|| format!("Failed copying source artifact {}", source.display()))?;
    temporary.as_file().sync_all()?;
    Ok(temporary)
}

fn persist_replace(temporary: NamedTempFile, destination: &Path) -> Result<(), Error> {
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "Failed atomically publishing artifact {}",
                destination.display()
            )
        })?;
    Ok(())
}

fn copy_file_atomically(source: &Path, destination: &Path) -> Result<(), Error> {
    let directory = destination.parent().with_context(|| {
        format!(
            "Artifact destination has no parent directory: {}",
            destination.display()
        )
    })?;
    persist_replace(copy_to_temporary(source, directory)?, destination)
}

fn verify_content_addressed_file(source: &Path, destination: &Path) -> Result<(), Error> {
    let source_bytes = fs::read(source)
        .with_context(|| format!("Failed reading source shard {}", source.display()))?;
    let destination_bytes = fs::read(destination).with_context(|| {
        format!(
            "Failed reading existing immutable shard {}",
            destination.display()
        )
    })?;
    if source_bytes != destination_bytes {
        bail!(
            "existing content-addressed shard has different bytes: {}",
            destination.display()
        );
    }
    Ok(())
}

fn copy_content_addressed_file(source: &Path, destination: &Path) -> Result<(), Error> {
    if !destination
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(storage::SHARD_FILE_SUFFIX))
    {
        bail!(
            "refusing to publish non-shard content-addressed artifact {}",
            destination.display()
        );
    }
    if destination.try_exists()? {
        return verify_content_addressed_file(source, destination);
    }
    let directory = destination.parent().with_context(|| {
        format!(
            "Shard destination has no parent directory: {}",
            destination.display()
        )
    })?;
    let temporary = copy_to_temporary(source, directory)?;
    match temporary.persist_noclobber(destination) {
        Ok(_file) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            verify_content_addressed_file(source, destination)
        }
        Err(error) => Err(error.error).with_context(|| {
            format!(
                "Failed atomically publishing shard {}",
                destination.display()
            )
        }),
    }
}

fn write_atomically(destination: &Path, contents: &[u8]) -> Result<(), Error> {
    let directory = destination.parent().with_context(|| {
        format!(
            "Artifact destination has no parent directory: {}",
            destination.display()
        )
    })?;
    let mut temporary = NamedTempFile::new_in(directory).with_context(|| {
        format!(
            "Failed creating temporary artifact in {}",
            directory.display()
        )
    })?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    persist_replace(temporary, destination)
}

fn remove_stale_root(directory: &Path) -> Result<(), Error> {
    let root_path = directory.join(storage::ROOT_FILENAME);
    if root_path.try_exists()? {
        fs::remove_file(&root_path).with_context(|| {
            format!(
                "Failed removing stale root artifact {}",
                root_path.display()
            )
        })?;
    }
    Ok(())
}

fn print_wasm_artifact_sizes(wasm_bytes: u64, artifacts: &storage::ShardedArtifacts) {
    println!("Artifact sizes:");
    println!("  WASM: {wasm_bytes} bytes");
    println!("  root: {} bytes", artifacts.root_bytes);
    println!(
        "  shards: {} files, {} bytes total, {} bytes max",
        artifacts.shard_files.len(),
        artifacts.total_shard_bytes,
        artifacts.max_shard_bytes
    );
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

/// Runs a child process and checks that it exits successfully.
///
/// # Errors
///
/// Returns an error if the process cannot be started or exits unsuccessfully.
fn run_command(cmd: &mut Command) -> Result<(), Error> {
    println!("running {cmd:?}");
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {cmd:?}"))?;

    if !status.success() {
        anyhow::bail!("failed to execute {cmd:?}\nstatus: {status}")
    }
    Ok(())
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    #[test]
    fn refuses_to_reuse_a_content_address_with_different_bytes() -> Result<(), Error> {
        let directory = TempDir::new()?;
        let filename = format!("{}{}", "a".repeat(64), storage::SHARD_FILE_SUFFIX);
        let source = directory.path().join("source");
        let destination = directory.path().join(filename);
        fs::write(&source, b"new shard bytes")?;
        fs::write(&destination, b"different existing bytes")?;

        assert!(copy_content_addressed_file(&source, &destination).is_err());
        assert_eq!(fs::read(destination)?, b"different existing bytes");
        Ok(())
    }
}

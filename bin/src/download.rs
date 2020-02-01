use failure::{err_msg, Error};
use semver::Version;
use serde_json::Value as Json;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use reqwest::header::CONTENT_LENGTH;
const CRATES_API_ROOT: &'static str = "https://crates.io/api/v1/crates";

/// Download given crate and return it as a vector of gzipped bytes.
fn download_crate(name: &str, version: &Version) -> Result<Vec<u8>, Error> {
    let download_url = format!("{}/{}/{}/download", CRATES_API_ROOT, name, version);
    debug!(
        "Downloading crate `{}=={}` from {}",
        name, version, download_url
    );
    let mut response = reqwest::blocking::get(&download_url)?;

    let content_length: Option<usize> = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|ct_len| ct_len.to_str().ok())
        .and_then(|ct_len| ct_len.parse().ok());
    trace!(
        "Download size: {}",
        content_length.map_or("<unknown>".into(), |cl| format!("{} bytes", cl))
    );
    let mut bytes = match content_length {
        Some(cl) => Vec::with_capacity(cl),
        None => Vec::new(),
    };
    response.read_to_end(&mut bytes)?;

    info!("Crate `{}=={}` downloaded successfully", name, version);
    Ok(bytes)
}

/// Talk to crates.io to get the newest version of given crate
/// that matches specified version requirements.
fn get_newest_version(crate_: String) -> Result<Version, Error> {
    let versions_url = format!("{}/{}/versions", CRATES_API_ROOT, crate_);
    debug!(
        "Fetching latest matching version of crate `{}` from {}",
        crate_, versions_url
    );
    let response: Json = reqwest::blocking::get(&versions_url)?.json()?;

    // TODO: rather than silently skipping over incorrect versions,
    // report them as malformed response from crates.io
    let mut versions = response
        .pointer("/versions")
        .and_then(|vs| vs.as_array())
        .map(|vs| {
            vs.iter()
                .filter_map(|v| {
                    v.as_object()
                        .and_then(|v| v.get("num"))
                        .and_then(|n| n.as_str())
                })
                .filter_map(|v| Version::parse(v).ok())
                .collect::<Vec<_>>()
        })
        .ok_or_else(|| err_msg(format!("malformed response from {}", versions_url)))?;

    if versions.is_empty() {
        failure::bail!("no valid versions found");
    }

    versions.sort_by(|a, b| b.cmp(a));
    Ok(versions
        .first()
        .expect("Cannot find any version of crate")
        .to_owned())
}

pub fn download_engine(dir: &Path) -> Result<PathBuf, Error> {
    let version = get_newest_version("tinysearch-engine".to_string())?;
    let crate_bytes = download_crate("tinysearch-engine", &version).expect("Cannot download crate");

    // Extract to a directory named $CRATE-$VERSION
    // Due to how crate archives are structured (they contain
    // single top-level directory) this is done automatically
    // if you simply extract them in $CWD.
    let gzip = flate2::read::GzDecoder::new(&crate_bytes[..]);
    let mut archive = tar::Archive::new(gzip);
    archive.unpack(dir)?;
    // If -x option was passed, we need to move the extracted directory
    // to wherever the user wanted.
    // fs::rename(&dir, &p)?;
    // Ok(p)
    let dir: PathBuf = format!("{}/{}-{}", dir.display(), "tinysearch-engine", version).into();
    Ok(dir)
}

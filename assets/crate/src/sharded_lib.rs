//! Raw WebAssembly ABI for a corpus-independent sharded tinysearch engine.
//!
//! The generated module owns one [`ShardedIndex`] and a table of JSON response
//! buffers. JavaScript passes byte slices as pointer/length pairs allocated by
//! [`engine_alloc`] and reads responses through opaque handles.

use std::alloc::{Layout, alloc, dealloc};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ptr;
use std::slice;
use std::str;

use serde_json::{Value, json};
use tinysearch::{ShardError, ShardId, ShardedIndex};

const ENGINE_ABI_VERSION: u32 = 1;

type ResultHandle = u32;

struct EngineState {
    index: Option<ShardedIndex>,
    results: BTreeMap<ResultHandle, Box<[u8]>>,
    next_handle: ResultHandle,
}

impl EngineState {
    fn new() -> Self {
        Self {
            index: None,
            results: BTreeMap::new(),
            next_handle: 1,
        }
    }

    fn insert_result(&mut self, bytes: Box<[u8]>) -> ResultHandle {
        if self.results.len() >= u32::MAX as usize {
            return 0;
        }

        loop {
            let handle = self.next_handle.max(1);
            self.next_handle = handle.wrapping_add(1).max(1);
            if let std::collections::btree_map::Entry::Vacant(entry) = self.results.entry(handle) {
                entry.insert(bytes);
                return handle;
            }
        }
    }
}

thread_local! {
    static ENGINE: RefCell<EngineState> = RefCell::new(EngineState::new());
}

fn error_response(code: &str, error: impl std::fmt::Display) -> Value {
    json!({
        "ok": false,
        "code": code,
        "error": error.to_string(),
    })
}

fn shard_list(index: &ShardedIndex, ids: &[ShardId]) -> Value {
    Value::Array(
        ids.iter()
            .filter_map(|id| {
                index
                    .descriptors()
                    .iter()
                    .find(|descriptor| descriptor.id == *id)
                    .map(|descriptor| {
                        json!({
                            "id": descriptor.id.get(),
                            "filename": descriptor.filename,
                        })
                    })
            })
            .collect(),
    )
}

fn store_response(response: Value) -> ResultHandle {
    let bytes = serde_json::to_vec(&response).unwrap_or_else(|error| {
        format!(
            "{{\"ok\":false,\"code\":\"json_serialization\",\"error\":{}}}",
            serde_json::to_string(&error.to_string())
                .unwrap_or_else(|_| "\"failed to serialize engine response\"".to_owned())
        )
        .into_bytes()
    });
    ENGINE.with_borrow_mut(|state| state.insert_result(bytes.into_boxed_slice()))
}

/// Borrows an ABI input region.
///
/// # Safety
///
/// For nonzero `len`, `ptr` must point to `len` initialized bytes in this
/// module's linear memory. The region must remain live and unmodified for the
/// duration of the call. Callers satisfy this by using [`engine_alloc`],
/// writing exactly `len` bytes, and calling [`engine_dealloc`] afterwards.
unsafe fn input_bytes<'input>(ptr: *const u8, len: usize) -> Result<&'input [u8], Value> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(error_response(
            "invalid_pointer",
            "a null input pointer was supplied with a nonzero length",
        ));
    }

    // SAFETY: The raw ABI caller must uphold the allocation, bounds, lifetime,
    // and initialization invariants documented above.
    Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

/// Decodes one UTF-8 ABI input without accepting replacement characters.
///
/// # Safety
///
/// The caller must uphold the same memory invariants as [`input_bytes`].
unsafe fn input_string<'input>(ptr: *const u8, len: usize) -> Result<&'input str, Value> {
    // SAFETY: Forwarded directly from this function's contract.
    let bytes = unsafe { input_bytes(ptr, len) }?;
    str::from_utf8(bytes)
        .map_err(|error| error_response("invalid_utf8", format!("query is not UTF-8: {error}")))
}

/// Returns the raw engine ABI version.
#[unsafe(no_mangle)]
pub extern "C" fn engine_abi_version() -> u32 {
    ENGINE_ABI_VERSION
}

/// Allocates `len` bytes in WASM linear memory for an ABI input.
///
/// Returns null for a zero length, an invalid layout, or allocation failure.
#[unsafe(no_mangle)]
pub extern "C" fn engine_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return ptr::null_mut();
    }
    let Ok(layout) = Layout::array::<u8>(len) else {
        return ptr::null_mut();
    };

    // SAFETY: `layout` is nonzero and valid. Ownership of the returned region
    // crosses the raw ABI and must be returned to `engine_dealloc` with `len`.
    unsafe { alloc(layout) }
}

/// Frees an input region returned by [`engine_alloc`].
///
/// # Safety
///
/// For nonzero `len`, `ptr` must be the still-live pointer returned by exactly
/// one `engine_alloc(len)` call and must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let Ok(layout) = Layout::array::<u8>(len) else {
        return;
    };

    // SAFETY: The raw ABI caller must uphold the matching-allocation invariant
    // documented above.
    unsafe { dealloc(ptr, layout) };
}

/// Decodes and installs a new root index, returning a JSON response handle.
///
/// A successful replacement discards loaded shards and outstanding response
/// buffers from the previous generation. Result handle allocation keeps
/// advancing so stale handles cannot alias new responses after a reload. A
/// malformed replacement leaves the current valid index intact.
///
/// # Safety
///
/// `ptr` and `len` must satisfy the input invariants documented on
/// [`input_bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_load_root(ptr: *const u8, len: usize) -> ResultHandle {
    // SAFETY: Forwarded directly from this function's contract.
    let bytes = match unsafe { input_bytes(ptr, len) } {
        Ok(bytes) => bytes,
        Err(response) => return store_response(response),
    };

    match ShardedIndex::from_root_bytes(bytes) {
        Ok(index) => {
            let shard_count = index.descriptors().len();
            ENGINE.with_borrow_mut(|state| {
                state.index = Some(index);
                state.results.clear();
            });
            store_response(json!({
                "ok": true,
                "shardCount": shard_count,
                "loadedShardCount": 0,
                "loadedShardBytes": 0,
            }))
        }
        Err(error) => store_response(error_response("invalid_root", error)),
    }
}

/// Plans all lexical shards needed for a UTF-8 query.
///
/// # Safety
///
/// `ptr` and `len` must satisfy the input invariants documented on
/// [`input_bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_plan_query(ptr: *const u8, len: usize) -> ResultHandle {
    // SAFETY: Forwarded directly from this function's contract.
    let query = match unsafe { input_string(ptr, len) } {
        Ok(query) => query,
        Err(response) => return store_response(response),
    };

    let response = ENGINE.with_borrow(|state| match state.index.as_ref() {
        Some(index) => {
            let required = index.required_shards(query);
            json!({
                "ok": true,
                "required": shard_list(index, &required),
                "loadedShardCount": index.loaded_shard_count(),
                "loadedShardBytes": index.loaded_shard_bytes(),
            })
        }
        None => error_response("not_initialized", "no sharded root has been loaded"),
    });
    store_response(response)
}

/// Validates and installs one lexical shard from its encoded bytes.
///
/// # Safety
///
/// `ptr` and `len` must satisfy the input invariants documented on
/// [`input_bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_load_shard(ptr: *const u8, len: usize) -> ResultHandle {
    // SAFETY: Forwarded directly from this function's contract.
    let bytes = match unsafe { input_bytes(ptr, len) } {
        Ok(bytes) => bytes,
        Err(response) => return store_response(response),
    };

    let response = ENGINE.with_borrow_mut(|state| {
        let Some(index) = state.index.as_mut() else {
            return error_response("not_initialized", "no sharded root has been loaded");
        };

        match index.load_shard(bytes) {
            Ok(id) => {
                let filename = index
                    .descriptors()
                    .iter()
                    .find(|descriptor| descriptor.id == id)
                    .map(|descriptor| descriptor.filename.as_str());
                json!({
                    "ok": true,
                    "id": id.get(),
                    "filename": filename,
                    "loadedShardCount": index.loaded_shard_count(),
                    "loadedShardBytes": index.loaded_shard_bytes(),
                })
            }
            Err(error) => error_response("invalid_shard", error),
        }
    });
    store_response(response)
}

/// Searches the currently loaded shard set and returns a JSON response handle.
///
/// If shards are missing, the error response includes their IDs and filenames
/// in `needs`; malformed UTF-8 and malformed state are also returned as JSON.
///
/// # Safety
///
/// `ptr` and `len` must satisfy the input invariants documented on
/// [`input_bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_search(ptr: *const u8, len: usize, limit: usize) -> ResultHandle {
    // SAFETY: Forwarded directly from this function's contract.
    let query = match unsafe { input_string(ptr, len) } {
        Ok(query) => query,
        Err(response) => return store_response(response),
    };

    let response = ENGINE.with_borrow(|state| {
        let Some(index) = state.index.as_ref() else {
            return error_response("not_initialized", "no sharded root has been loaded");
        };

        match index.search(query, limit) {
            Ok(posts) => {
                let results: Vec<Value> = posts
                    .into_iter()
                    .map(|post| {
                        json!({
                            "title": post.title,
                            "url": post.url,
                            "meta": post.meta,
                        })
                    })
                    .collect();
                json!({
                    "ok": true,
                    "results": results,
                    "loadedShardCount": index.loaded_shard_count(),
                    "loadedShardBytes": index.loaded_shard_bytes(),
                })
            }
            Err(ShardError::NeedsShards(ids)) => json!({
                "ok": false,
                "code": "needs_shards",
                "error": "query requires lexical shards that are not loaded",
                "needs": shard_list(index, &ids),
            }),
            Err(error) => error_response("search_failed", error),
        }
    });
    store_response(response)
}

/// Returns the pointer for an outstanding JSON response handle.
///
/// The pointer remains valid until [`engine_result_free`] is called for the
/// handle. Unknown handles return null.
#[unsafe(no_mangle)]
pub extern "C" fn engine_result_ptr(handle: ResultHandle) -> *const u8 {
    ENGINE.with_borrow(|state| {
        state
            .results
            .get(&handle)
            .map_or(ptr::null(), |bytes| bytes.as_ptr())
    })
}

/// Returns the byte length for an outstanding JSON response handle.
///
/// Unknown handles return zero.
#[unsafe(no_mangle)]
pub extern "C" fn engine_result_len(handle: ResultHandle) -> usize {
    ENGINE.with_borrow(|state| state.results.get(&handle).map_or(0, |bytes| bytes.len()))
}

/// Releases an opaque JSON response handle.
#[unsafe(no_mangle)]
pub extern "C" fn engine_result_free(handle: ResultHandle) {
    ENGINE.with_borrow_mut(|state| {
        state.results.remove(&handle);
    });
}

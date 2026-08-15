use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::OnceLock;

use tinysearch::{PostId, SearchIndex, Storage, search as base_search};

static SEARCH_INDEX: OnceLock<SearchIndex> = OnceLock::new();

/// Allocates an input buffer for the generated JavaScript loader.
#[unsafe(no_mangle)]
pub extern "C" fn alloc_query(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    Box::into_raw(vec![0_u8; len].into_boxed_slice()).cast::<u8>()
}

/// Releases an input buffer returned by [`alloc_query`].
///
/// # Safety
///
/// `ptr` and `len` must identify a still-live allocation returned by exactly one
/// `alloc_query(len)` call, and the pointer must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_query(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    let slice = std::ptr::slice_from_raw_parts_mut(ptr, len);
    // SAFETY: The caller must uphold the matching-allocation contract above.
    drop(unsafe { Box::from_raw(slice) });
}

pub fn search_local(query: String, num_results: usize) -> Vec<&'static PostId> {
    let index = SEARCH_INDEX.get_or_init(|| {
        let bytes = include_bytes!("storage");
        Storage::from_bytes(bytes).unwrap().filters
    });
    base_search(index, &query, num_results)
}

/// Export for WASM - search function that takes C strings and returns JSON
#[unsafe(no_mangle)]
pub extern "C" fn search(query_ptr: *const c_char, num_results: usize) -> *mut c_char {
    if query_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let query_cstr = unsafe { CStr::from_ptr(query_ptr) };
    let query = match query_cstr.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return std::ptr::null_mut(),
    };

    let results = search_local(query, num_results);

    // Convert results to a simple JSON format
    let json_results: Vec<serde_json::Value> = results
        .into_iter()
        .map(|post_id| {
            serde_json::json!({
                "title": post_id.title,
                "url": post_id.url,
                "meta": post_id.meta
            })
        })
        .collect();

    let json_string = match serde_json::to_string(&json_results) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json_string) {
        Ok(cstring) => cstring.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free memory allocated by search function
#[unsafe(no_mangle)]
pub extern "C" fn free_search_result(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

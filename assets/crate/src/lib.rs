use std::sync::OnceLock;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use tinysearch::{search as base_search, Filters, PostId, Storage};

static FILTERS: OnceLock<Filters> = OnceLock::new();

pub fn search_local(query: String, num_results: usize) -> Vec<&'static PostId> {
    let filters = FILTERS.get_or_init(|| {
        let bytes = include_bytes!("storage");
        Storage::from_bytes(bytes).unwrap().filters
    });
    base_search(filters, query, num_results)
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
        .map(|post_id| serde_json::json!({
            "title": post_id.0,
            "url": post_id.1,
            "meta": post_id.2
        }))
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

pub static CRATE_CARGO_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/crate/Cargo_orig.toml"
));
pub static CRATE_LIB_RS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/crate/src/lib.rs"
));

// Include a bare-bones HTML page template that demonstrates how tinysearch is used
pub static DEMO_HTML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/demo.html"));

pub static STOP_WORDS: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/stopwords"));

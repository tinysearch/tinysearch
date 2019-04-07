pub fn valid(s: &str) -> bool {
    s.len() > 1
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

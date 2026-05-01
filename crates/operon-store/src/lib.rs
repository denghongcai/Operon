pub const DEFAULT_STORE_PATH: &str = "operon.db";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_path_is_stable() {
        assert_eq!(DEFAULT_STORE_PATH, "operon.db");
    }
}

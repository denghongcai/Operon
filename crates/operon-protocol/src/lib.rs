pub const PROTOCOL_VERSION: &str = "v0.1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_matches_mvp_release_line() {
        assert_eq!(PROTOCOL_VERSION, "v0.1");
    }
}

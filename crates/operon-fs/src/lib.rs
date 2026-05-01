pub const FILESYSTEM_CAPABILITY: &str = "fs";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_capability_id_is_stable() {
        assert_eq!(FILESYSTEM_CAPABILITY, "fs");
    }
}

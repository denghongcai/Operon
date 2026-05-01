pub const MOUNT_CAPABILITY: &str = "mount";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_capability_id_is_stable() {
        assert_eq!(MOUNT_CAPABILITY, "mount");
    }
}

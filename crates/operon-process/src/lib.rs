pub const PROCESS_CAPABILITY: &str = "process";
pub const JOB_CAPABILITY: &str = "job";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_capability_ids_are_stable() {
        assert_eq!(PROCESS_CAPABILITY, "process");
        assert_eq!(JOB_CAPABILITY, "job");
    }
}

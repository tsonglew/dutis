use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
struct GroupConfig {
    groups: HashMap<String, Vec<String>>,
}

lazy_static! {
    static ref GROUPS: GroupConfig = {
        let config_path = "config/groups.yaml";
        let contents =
            fs::read_to_string(config_path).expect("Failed to read groups configuration file");
        serde_yaml::from_str(&contents).expect("Failed to parse groups configuration")
    };
}

pub fn get_suffix_group(group_name: &str) -> Option<Vec<&str>> {
    GROUPS
        .groups
        .get(group_name)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
}

pub fn list_available_groups() -> Vec<&'static str> {
    GROUPS.groups.keys().map(|s| s.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_suffix_group() {
        // Test valid group
        let video_group = get_suffix_group("video");
        assert!(video_group.is_some());
        assert!(video_group.unwrap().contains(&"mp4"));

        // Test invalid group
        let invalid_group = get_suffix_group("invalid");
        assert!(invalid_group.is_none());
    }
}

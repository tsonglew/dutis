use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use libc::{sleep, ATTR_VOL_ALLOCATIONCLUMP};
use std::collections::HashSet;
use std::ffi::c_void;
use std::path::Path;
use std::thread;
use std::time::Duration;

mod names;
pub use names::{get_common_suffix, get_friendly_name};

use crate::{BiMap, Config};

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    // Note: These CoreServices APIs are deprecated since macOS 10.4–12.0.
    // Apple has not provided direct replacements for these functionalities.
    // While they continue to work, they may become unstable in future macOS versions.
    
    fn UTTypeCreatePreferredIdentifierForTag(
        inTagClass: CFStringRef,
        inTag: CFStringRef,
        inConformingToUTI: CFStringRef,
    ) -> CFStringRef;

    fn LSSetDefaultRoleHandlerForContentType(
        inContentType: CFStringRef,
        inRole: CFStringRef,
        inHandlerBundleID: CFStringRef,
    ) -> i32;

    fn LSCopyAllRoleHandlersForContentType(
        inContentType: CFStringRef,
        inRole: CFStringRef,
    ) -> CFArrayRef;
}

pub fn get_uti_from_suffix(suffix: &str) -> Option<String> {
    if suffix.is_empty() {
        return None;
    }

    unsafe {
        let cf_tag_class = CFString::new("public.filename-extension");
        let cf_tag = CFString::new(suffix);
        let cf_conforming_to = CFString::new("");

        let uti_ref = UTTypeCreatePreferredIdentifierForTag(
            cf_tag_class.as_concrete_TypeRef(),
            cf_tag.as_concrete_TypeRef(),
            cf_conforming_to.as_concrete_TypeRef(),
        );

        if uti_ref.is_null() {
            None
        } else {
            let cf_uti = CFString::wrap_under_create_rule(uti_ref);
            Some(cf_uti.to_string())
        }
    }
}

pub fn set_default_app_for_suffix(config: &Config, bundle_id: &str) -> Result<(), String> {
    if config.suffix.is_empty() {
        return Err("Suffix cannot be empty".to_string());
    }

    if bundle_id.is_empty() {
        return Err("Bundle ID cannot be empty".to_string());
    }

    unsafe {
        let cf_uti = CFString::new(&config.uti);
        let cf_role = CFString::new("all");
        let cf_bundle_id = CFString::new(bundle_id);

        let attempts: i32 = 200; // Number of attempts to make the change stable

        let mut result: i32 = 0;
        for _ in 0..attempts {
            result = LSSetDefaultRoleHandlerForContentType(
                cf_uti.as_concrete_TypeRef(),
                cf_role.as_concrete_TypeRef(),
                cf_bundle_id.as_concrete_TypeRef(),
            );
            thread::sleep(Duration::from_millis(10));
        }

        if result == 0 {
            Ok(())
        } else {
            Err(format!(
                "Failed to set default application. Error code: {}",
                result
            ))
        }
    }
}

pub fn get_role_handlers_from_uti(uti: &str) -> Option<Vec<String>> {
    if uti.is_empty() {
        return None;
    }

    // Try multiple times to get a stable list of handlers
    let mut all_handlers = std::collections::HashSet::new();
    let attempts: i32 = 200; // Number of attempts to get a stable list

    for _ in 0..attempts {
        unsafe {
            let cf_uti = CFString::new(uti);
            let cf_role = CFString::new("all");

            let handlers_ref = LSCopyAllRoleHandlersForContentType(
                cf_uti.as_concrete_TypeRef(),
                cf_role.as_concrete_TypeRef(),
            );

            if handlers_ref.is_null() {
                continue; // Try again if we got null
            }

            let handlers = CFArray::<CFString>::wrap_under_create_rule(handlers_ref);
            let count = handlers.len();

            for i in 0..count {
                if let Some(handler) = handlers.get(i) {
                    all_handlers.insert(handler.to_string());
                }
            }
        }

        // Small delay between attempts
        thread::sleep(Duration::from_millis(10));
    }

    // Return None if no handlers were found after all attempts
    if all_handlers.is_empty() {
        None
    } else {
        // Convert HashSet to Vec and sort for stable output
        let mut result: Vec<String> = all_handlers.into_iter().collect();
        result.sort();
        Some(result)
    }
}

pub fn get_common_role_handlers(uti2suf: &BiMap<String, String>) -> Option<Vec<String>> {
    let mut common_handlers: Option<Vec<String>> = None;

    for (uti, _) in uti2suf.iter() {
        if let Some(handlers) = get_role_handlers_from_uti(uti) {
            match &mut common_handlers {
                None => common_handlers = Some(handlers),
                Some(common) => {
                    common.retain(|h| handlers.contains(h));
                    if common.is_empty() {
                        return None;
                    }
                }
            }
        } else {
            return None;
        }
    }

    common_handlers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_uti_from_suffix() {
        // Test valid suffixes
        assert!(get_uti_from_suffix("html").is_some());
        assert!(get_uti_from_suffix("txt").is_some());
        assert!(get_uti_from_suffix("jpg").is_some());
        assert!(get_uti_from_suffix("pdf").is_some());
        assert!(get_uti_from_suffix("rs").is_some());

        // Test empty suffix
        assert!(get_uti_from_suffix("").is_none());

        // Test invalid suffix
        assert!(get_uti_from_suffix("nonexistentsuffix123456789").is_some()); // Even invalid suffixes return a UTI
    }

    #[test]
    fn test_set_default_app_for_suffix() {
        let config = Config {
            suffix: "txt".to_string(),
            uti: "public.plain-text".to_string(),
        };

        // Test empty bundle_id
        let result = set_default_app_for_suffix(&config, "");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Bundle ID cannot be empty");

        // Test empty suffix in config
        let empty_config = Config {
            suffix: "".to_string(),
            uti: "public.plain-text".to_string(),
        };
        let result = set_default_app_for_suffix(&empty_config, "com.apple.textedit");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Suffix cannot be empty");

        // Test valid case with TextEdit (this might fail if TextEdit is not installed)
        let result = set_default_app_for_suffix(&config, "com.apple.textedit");
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_role_handlers_from_uti() {
        // Test empty UTI
        assert!(get_role_handlers_from_uti("").is_none());

        // Test valid UTI that should have handlers (like text files)
        let handlers1 = get_role_handlers_from_uti("public.plain-text");
        assert!(handlers1.is_some());
        let handlers1 = handlers1.unwrap();
        assert!(!handlers1.is_empty());

        // Test stability - multiple calls should return the same handlers
        let handlers2 = get_role_handlers_from_uti("public.plain-text").unwrap();
        assert_eq!(
            handlers1, handlers2,
            "Handler lists should be stable across calls"
        );

        // Test UTI that might not have handlers
        let handlers = get_role_handlers_from_uti("dyn.ah62d4sv4ge81g5pe"); // A dynamic UTI
        assert!(
            handlers.is_none(),
            "Dynamic UTIs typically don't have handlers"
        );

        // Test invalid UTI
        let handlers = get_role_handlers_from_uti("invalid.uti.identifier");
        assert!(handlers.is_none(), "Invalid UTIs should return None");
    }

    #[test]
    fn test_get_common_role_handlers() {
        // Create a test BiMap
        let mut uti2suf = BiMap::new();
        uti2suf.insert("public.plain-text".to_string(), "txt".to_string());
        uti2suf.insert("public.html".to_string(), "html".to_string());

        // Test with valid UTIs
        let common_handlers = get_common_role_handlers(&uti2suf);
        assert!(common_handlers.is_some());

        // Test with empty BiMap
        let empty_map = BiMap::new();
        let common_handlers = get_common_role_handlers(&empty_map);
        assert!(common_handlers.is_none()); // Empty map should return None
    }
}

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use libc::sleep;
use std::collections::HashSet;
use std::ffi::c_void;
use std::path::Path;

mod names;
pub use names::{get_common_suffix, get_friendly_name};

use crate::{BiMap, Config};

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
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

        let result = LSSetDefaultRoleHandlerForContentType(
            cf_uti.as_concrete_TypeRef(),
            cf_role.as_concrete_TypeRef(),
            cf_bundle_id.as_concrete_TypeRef(),
        );

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

    unsafe {
        let cf_uti = CFString::new(uti);
        let cf_role = CFString::new("all");

        let handlers_ref = LSCopyAllRoleHandlersForContentType(
            cf_uti.as_concrete_TypeRef(),
            cf_role.as_concrete_TypeRef(),
        );

        if handlers_ref.is_null() {
            return None;
        }

        let handlers = CFArray::<CFString>::wrap_under_create_rule(handlers_ref);
        let count = handlers.len();
        let mut result = Vec::with_capacity(count as usize);

        for i in 0..count {
            if let Some(handler) = handlers.get(i) {
                result.push(handler.to_string());
            }
        }

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
    fn test_common_suffixes() {
        // Test some common suffixes to ensure UTI lookup works
        assert!(get_uti_from_suffix("html").is_some());
        assert!(get_uti_from_suffix("txt").is_some());
        assert!(get_uti_from_suffix("jpg").is_some());
        assert!(get_uti_from_suffix("pdf").is_some());

        // Even uncommon suffixes should return Some UTI if the system recognizes them
        assert!(get_uti_from_suffix("rs").is_some());
    }
}

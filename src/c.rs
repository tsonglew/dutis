extern crate core_foundation;
extern crate objc;

use core_foundation::array::CFArray;
use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::string::CFString;
use core_services::{CFArrayRef, CFStringRef};

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSCopyAllRoleHandlersForContentType(
        inContentType: CFStringRef,
        inRole: CFStringRef,
    ) -> CFArrayRef;
}

pub fn get_all_role_handlers_for_content_type(
    content_type: &str,
    role: &str,
) -> Option<Vec<String>> {
    // Create CFStringRefs from Rust &str
    let cf_content_type = CFString::new(content_type);
    let cf_role = CFString::new(role);

    unsafe {
        let handlers: CFArrayRef = LSCopyAllRoleHandlersForContentType(
            cf_content_type.as_concrete_TypeRef(),
            cf_role.as_concrete_TypeRef(),
        );

        if handlers.is_null() {
            return None;
        }

        // Convert CFArrayRef to Vec<String>
        let cf_array = CFArray::from_void(handlers as *const std::ffi::c_void);
        let handlers_count = cf_array.len();
        let mut result = Vec::with_capacity(handlers_count);

        for handler in cf_array.iter() {
            let cf_handler: CFString = TCFType::wrap_under_get_rule(handler as CFTypeRef);
            result.push(cf_handler.to_string());
        }

        Some(result)
    }
}

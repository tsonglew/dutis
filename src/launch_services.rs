use crate::association::{AssociationKind, AssociationTarget, HandlerRole};
use anyhow::{bail, Result};
use serde::Serialize;

pub const NATIVE_DEFAULTS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NativeDefaultHandler {
    pub bundle_id: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NativeRoleDefault {
    pub role: HandlerRole,
    pub bundle_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NativeDefaultsReport {
    pub schema_version: u32,
    pub kind: AssociationKind,
    pub identifier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub defaults: Vec<NativeRoleDefault>,
}

pub fn query_default_handler(
    association: &AssociationTarget,
) -> Result<Option<NativeDefaultHandler>> {
    platform::query_default_handler(association)
}

pub fn query_role_defaults(
    kind: AssociationKind,
    identifier: &str,
) -> Result<NativeDefaultsReport> {
    let target = AssociationTarget::new(kind, identifier, HandlerRole::All)?;
    platform::query_role_defaults(&target)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use anyhow::{Context, Result};
    use std::ffi::{c_char, c_void, CStr};
    use std::ptr;

    type CFIndex = isize;
    type CFStringEncoding = u32;
    type CFStringRef = *const c_void;
    type LSRolesMask = u32;

    const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x0800_0100;
    const K_LS_ROLES_VIEWER: LSRolesMask = 0x0000_0002;
    const K_LS_ROLES_EDITOR: LSRolesMask = 0x0000_0004;
    const K_LS_ROLES_SHELL: LSRolesMask = 0x0000_0008;
    const K_LS_ROLES_ALL: LSRolesMask = 0xffff_ffff;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            num_bytes: CFIndex,
            encoding: CFStringEncoding,
            is_external_representation: u8,
        ) -> CFStringRef;
        fn CFStringGetLength(string: CFStringRef) -> CFIndex;
        fn CFStringGetMaximumSizeForEncoding(
            length: CFIndex,
            encoding: CFStringEncoding,
        ) -> CFIndex;
        fn CFStringGetCString(
            string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: CFIndex,
            encoding: CFStringEncoding,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        static kUTTagClassFilenameExtension: CFStringRef;
        static kUTTagClassMIMEType: CFStringRef;

        fn UTTypeCreatePreferredIdentifierForTag(
            tag_class: CFStringRef,
            tag: CFStringRef,
            conforming_to: CFStringRef,
        ) -> CFStringRef;
        fn LSCopyDefaultRoleHandlerForContentType(
            content_type: CFStringRef,
            role: LSRolesMask,
        ) -> CFStringRef;
        fn LSCopyDefaultHandlerForURLScheme(scheme: CFStringRef) -> CFStringRef;
    }

    struct OwnedCfString(CFStringRef);

    impl OwnedCfString {
        fn new(value: &str) -> Result<Self> {
            let length = CFIndex::try_from(value.len()).context("string is too long")?;
            // SAFETY: The byte pointer is valid for `length` bytes for the duration of the call.
            // Core Foundation copies the UTF-8 bytes into the returned owned string.
            let string = unsafe {
                CFStringCreateWithBytes(
                    ptr::null(),
                    value.as_ptr(),
                    length,
                    K_CF_STRING_ENCODING_UTF8,
                    0,
                )
            };
            Self::from_create_rule(string).context("Core Foundation could not create a string")
        }

        fn from_create_rule(value: CFStringRef) -> Option<Self> {
            (!value.is_null()).then(|| Self(value))
        }

        fn as_ptr(&self) -> CFStringRef {
            self.0
        }

        fn to_string(&self) -> Result<String> {
            cf_string_to_string(self.0)
        }
    }

    impl Drop for OwnedCfString {
        fn drop(&mut self) {
            // SAFETY: This wrapper only contains values returned under Core Foundation's create
            // rule, and it releases each value exactly once.
            unsafe { CFRelease(self.0) };
        }
    }

    fn cf_string_to_string(string: CFStringRef) -> Result<String> {
        // SAFETY: Callers provide a live CFStringRef.
        let length = unsafe { CFStringGetLength(string) };
        // SAFETY: The encoding constant is valid and `length` came from the same string.
        let maximum =
            unsafe { CFStringGetMaximumSizeForEncoding(length, K_CF_STRING_ENCODING_UTF8) };
        let capacity = maximum
            .checked_add(1)
            .context("Core Foundation string is too large")?;
        let mut buffer = vec![0_u8; usize::try_from(capacity)?];
        // SAFETY: The allocated buffer has `capacity` bytes and the API writes a NUL-terminated
        // UTF-8 representation when it returns true.
        let converted = unsafe {
            CFStringGetCString(
                string,
                buffer.as_mut_ptr().cast(),
                capacity,
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        if converted == 0 {
            bail!("Core Foundation could not encode a string as UTF-8");
        }
        // SAFETY: A successful CFStringGetCString call guarantees NUL termination.
        Ok(unsafe { CStr::from_ptr(buffer.as_ptr().cast()) }
            .to_str()
            .context("Core Foundation returned invalid UTF-8")?
            .to_owned())
    }

    pub(super) fn query_default_handler(
        association: &AssociationTarget,
    ) -> Result<Option<NativeDefaultHandler>> {
        if association.kind == AssociationKind::UrlScheme {
            let scheme = OwnedCfString::new(&association.identifier)?;
            // SAFETY: `scheme` is a valid CFStringRef for the duration of the call. A non-null
            // result follows the create rule and is transferred into OwnedCfString.
            let result = unsafe { LSCopyDefaultHandlerForURLScheme(scheme.as_ptr()) };
            return owned_string_value(result).map(|bundle_id| {
                bundle_id.map(|bundle_id| NativeDefaultHandler {
                    bundle_id,
                    content_type: None,
                })
            });
        }

        let content_type = content_type_for(association)?;
        let content_type_name = content_type.to_string()?;
        query_content_type_handler(&content_type, association.role).map(|bundle_id| {
            bundle_id.map(|bundle_id| NativeDefaultHandler {
                bundle_id,
                content_type: Some(content_type_name),
            })
        })
    }

    pub(super) fn query_role_defaults(
        association: &AssociationTarget,
    ) -> Result<NativeDefaultsReport> {
        let roles: &[HandlerRole] = if association.kind == AssociationKind::UrlScheme {
            &[HandlerRole::All]
        } else {
            &[
                HandlerRole::All,
                HandlerRole::Viewer,
                HandlerRole::Editor,
                HandlerRole::Shell,
            ]
        };
        let (content_type, defaults) = if association.kind == AssociationKind::UrlScheme {
            let result = query_default_handler(association)?;
            (
                None,
                vec![NativeRoleDefault {
                    role: HandlerRole::All,
                    bundle_id: result.map(|handler| handler.bundle_id),
                }],
            )
        } else {
            let content_type = content_type_for(association)?;
            let content_type_name = content_type.to_string()?;
            let defaults = roles
                .iter()
                .map(|role| {
                    Ok(NativeRoleDefault {
                        role: *role,
                        bundle_id: query_content_type_handler(&content_type, *role)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            (Some(content_type_name), defaults)
        };
        Ok(NativeDefaultsReport {
            schema_version: NATIVE_DEFAULTS_SCHEMA_VERSION,
            kind: association.kind,
            identifier: association.identifier.clone(),
            content_type,
            defaults,
        })
    }

    fn query_content_type_handler(
        content_type: &OwnedCfString,
        role: HandlerRole,
    ) -> Result<Option<String>> {
        // SAFETY: `content_type` is valid for the duration of the call. A non-null result follows
        // the create rule and is transferred into OwnedCfString.
        let result = unsafe {
            LSCopyDefaultRoleHandlerForContentType(content_type.as_ptr(), role_mask(role))
        };
        owned_string_value(result)
    }

    fn content_type_for(association: &AssociationTarget) -> Result<OwnedCfString> {
        match association.kind {
            AssociationKind::Uti => OwnedCfString::new(&association.identifier),
            AssociationKind::Extension | AssociationKind::Mime => {
                let tag = OwnedCfString::new(&association.identifier)?;
                let data_type = OwnedCfString::new("public.data")?;
                // SAFETY: Framework constants and `tag` are valid CFStringRefs. A non-null result
                // follows the create rule and is transferred into OwnedCfString.
                let content_type = unsafe {
                    let tag_class = match association.kind {
                        AssociationKind::Extension => kUTTagClassFilenameExtension,
                        AssociationKind::Mime => kUTTagClassMIMEType,
                        AssociationKind::Uti | AssociationKind::UrlScheme => unreachable!(),
                    };
                    UTTypeCreatePreferredIdentifierForTag(
                        tag_class,
                        tag.as_ptr(),
                        data_type.as_ptr(),
                    )
                };
                OwnedCfString::from_create_rule(content_type)
                    .context("Launch Services could not resolve the identifier to a content type")
            }
            AssociationKind::UrlScheme => {
                unreachable!("URL schemes do not use content type queries")
            }
        }
    }

    fn owned_string_value(value: CFStringRef) -> Result<Option<String>> {
        let Some(value) = OwnedCfString::from_create_rule(value) else {
            return Ok(None);
        };
        let value = value.to_string()?;
        Ok((!value.is_empty()).then_some(value))
    }

    fn role_mask(role: HandlerRole) -> LSRolesMask {
        match role {
            HandlerRole::All => K_LS_ROLES_ALL,
            HandlerRole::Viewer => K_LS_ROLES_VIEWER,
            HandlerRole::Editor => K_LS_ROLES_EDITOR,
            HandlerRole::Shell => K_LS_ROLES_SHELL,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn core_foundation_string_round_trips_utf8() {
            let value = OwnedCfString::new("public.plain-text").unwrap();
            assert_eq!(value.to_string().unwrap(), "public.plain-text");
        }

        #[test]
        fn resolves_filename_extension_to_content_type() {
            // SAFETY: The framework exports this process-lifetime CFString constant.
            let tag_class = unsafe { kUTTagClassFilenameExtension };
            assert_eq!(
                cf_string_to_string(tag_class).unwrap(),
                "public.filename-extension"
            );
            let target = AssociationTarget::extension("txt").unwrap();
            let content_type = content_type_for(&target).unwrap().to_string().unwrap();
            assert!(!content_type.is_empty());
        }

        #[test]
        fn returns_complete_role_shapes_when_defaults_are_absent() {
            let report =
                query_role_defaults(&AssociationTarget::extension("txt").unwrap()).unwrap();
            assert_eq!(report.kind, AssociationKind::Extension);
            assert_eq!(report.identifier, "txt");
            assert!(report.content_type.is_some());
            assert_eq!(
                report
                    .defaults
                    .iter()
                    .map(|entry| entry.role)
                    .collect::<Vec<_>>(),
                [
                    HandlerRole::All,
                    HandlerRole::Viewer,
                    HandlerRole::Editor,
                    HandlerRole::Shell,
                ]
            );

            let url = AssociationTarget::new(AssociationKind::UrlScheme, "https", HandlerRole::All)
                .unwrap();
            let report = query_role_defaults(&url).unwrap();
            assert_eq!(report.defaults.len(), 1);
            assert_eq!(report.defaults[0].role, HandlerRole::All);
            assert!(report.content_type.is_none());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub(super) fn query_default_handler(
        _association: &AssociationTarget,
    ) -> Result<Option<NativeDefaultHandler>> {
        bail!("native Launch Services queries are available only on macOS")
    }

    pub(super) fn query_role_defaults(
        _association: &AssociationTarget,
    ) -> Result<NativeDefaultsReport> {
        bail!("native Launch Services queries are available only on macOS")
    }
}

//! Compare native declarations before publishing shader/CPU programs. This is
//! metadata agreement, not mathematical proof of transfer functions/primaries.
use super::*;
use crate::color_config::{ColorConfigManifest, ConfigColorSpace, ConfigOrigin, ReferenceSpace};
use std::collections::{BTreeMap, BTreeSet};

#[repr(C)]
#[derive(Default)]
pub(super) struct Row {
    kind: u32,
    reference: u32,
    data: u32,
    name: *const c_char,
    value: *const c_char,
}
pub(super) type Query =
    unsafe extern "C" fn(*const c_void, u32, *mut Row, *mut u32, *mut NativeError) -> bool;

fn text(pointer: *const c_char) -> Result<String> {
    if pointer.is_null() {
        return Err(Error::Admission("native catalogue returned a null string"));
    }
    // The hash-verified bridge bounds each owned string to 256 bytes and keeps
    // the immutable native Program alive for all borrowed catalogue rows.
    let value = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map_err(|_| Error::Admission("native catalogue string is not UTF-8"))?;
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err(Error::Admission(
            "native catalogue string exceeds name capacity",
        ));
    }
    Ok(value.into())
}
fn mismatch(
    field: &'static str,
    name: &str,
    expected: impl ToString,
    actual: impl ToString,
) -> Error {
    Error::Catalogue {
        field,
        name: name.into(),
        expected: expected.to_string(),
        actual: actual.to_string(),
    }
}
fn names<T, U>(
    field: &'static str,
    expected: &BTreeMap<String, T>,
    actual: &BTreeMap<String, U>,
) -> Result<()> {
    for name in expected.keys() {
        if !actual.contains_key(name) {
            return Err(mismatch(field, name, "present", "missing"));
        }
    }
    for name in actual.keys() {
        if !expected.contains_key(name) {
            return Err(mismatch(field, name, "absent", "unexpected"));
        }
    }
    Ok(())
}
pub(super) fn verify(program: &NativeColorProgram, manifest: &ColorConfigManifest) -> Result<()> {
    let mut spaces = BTreeMap::new();
    let mut roles = BTreeMap::new();
    let mut displays: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut builtin = None;
    let mut count = 1;
    let mut index = 0;
    while index < count {
        let mut row = Row::default();
        let mut total = 0;
        let mut error = NativeError::empty();
        let ok = unsafe {
            (program.api.catalogue)(
                program.raw.as_ptr(),
                index,
                &mut row,
                &mut total,
                &mut error,
            )
        };
        error.check(ok, "catalogue")?;
        if total == 0 || total > 2721 || (index > 0 && total != count) {
            return Err(Error::Admission(
                "native catalogue count is invalid or changed",
            ));
        }
        count = total;
        let name = text(row.name)?;
        let value = text(row.value)?;
        if name.is_empty() {
            return Err(Error::Admission("native catalogue name is empty"));
        }
        let duplicate = match row.kind {
            0 => {
                let reference = match row.reference {
                    0 => ReferenceSpace::Scene,
                    1 => ReferenceSpace::Display,
                    _ => return Err(Error::Admission("native catalogue reference is invalid")),
                };
                if row.data > 1 {
                    return Err(Error::Admission("native catalogue data flag is invalid"));
                }
                spaces
                    .insert(
                        name,
                        ConfigColorSpace {
                            encoding: value,
                            reference,
                            is_data: row.data != 0,
                        },
                    )
                    .is_some()
            }
            1 if row.reference == 0 && row.data == 0 => roles.insert(name, value).is_some(),
            2 if row.reference == 0 && row.data == 0 && value.is_empty() => {
                displays.insert(name, BTreeSet::new()).is_some()
            }
            3 if row.reference == 0 && row.data == 0 && !value.is_empty() => !displays
                .get_mut(&name)
                .ok_or(Error::Admission("native catalogue view has no display"))?
                .insert(value),
            4 if row.reference == 0 && row.data == 0 && name == "builtin" && !value.is_empty() => {
                builtin.replace(value).is_some()
            }
            _ => {
                return Err(Error::Admission(
                    "native catalogue row kind or flags are invalid",
                ))
            }
        };
        if duplicate {
            return Err(Error::Admission("native catalogue has duplicate entries"));
        }
        index += 1;
    }
    names("spaces", &manifest.spaces, &spaces)?;
    for (name, expected) in &manifest.spaces {
        let actual = &spaces[name];
        if expected.encoding != actual.encoding {
            return Err(mismatch(
                "encoding",
                name,
                &expected.encoding,
                &actual.encoding,
            ));
        }
        if expected.reference != actual.reference {
            return Err(mismatch(
                "reference",
                name,
                format!("{:?}", expected.reference),
                format!("{:?}", actual.reference),
            ));
        }
        if expected.is_data != actual.is_data {
            return Err(mismatch("is_data", name, expected.is_data, actual.is_data));
        }
    }
    names("roles", &manifest.roles, &roles)?;
    for (name, expected) in &manifest.roles {
        if &roles[name] != expected {
            return Err(mismatch("role_target", name, expected, &roles[name]));
        }
    }
    names("displays", &manifest.displays, &displays)?;
    for (display, expected) in &manifest.displays {
        let actual = &displays[display];
        for view in expected {
            if !actual.contains(view) {
                return Err(mismatch("views", display, view, "missing"));
            }
        }
        for view in actual {
            if !expected.contains(view) {
                return Err(mismatch("views", display, "absent", view));
            }
        }
    }
    if let ConfigOrigin::Builtin { id, aces } = &manifest.origin {
        let actual = builtin.as_deref().unwrap_or("unverified");
        if id != actual {
            return Err(mismatch("builtin_id", id, id, actual));
        }
        // OCIO's exact versioned registry IDs declare ACES as major.minor.
        // The native proof above must already match the full configuration.
        let version = actual
            .split_once("_aces-v")
            .and_then(|(_, suffix)| suffix.split_once("_ocio-v"))
            .and_then(|(aces, _)| aces.split_once('.'))
            .and_then(|(major, minor)| {
                Some(ColorVersion {
                    major: major.parse().ok()?,
                    minor: minor.parse().ok()?,
                    patch: 0,
                })
            })
            .ok_or(Error::Admission(
                "native builtin ACES version is not explicit",
            ))?;
        if *aces != version {
            let text = |v: ColorVersion| format!("{}.{}.{}", v.major, v.minor, v.patch);
            return Err(mismatch("aces_version", id, text(*aces), text(version)));
        }
    }
    Ok(())
}

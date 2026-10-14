//! JNI value conversion and the stable ok/data/error envelope.
use jni::{
    objects::{JByteBuffer, JObject, JString},
    sys::{jbyteArray, jint, jstring},
    JNIEnv,
};
use motion_host::ops::{dispatch, error_envelope};
use motion_host::Result;
use serde_json::Value;

/// Capture the original Throwable before clearing it. Java methods used to
/// format diagnostics may themselves throw, so each attempt has its own cleanup.
pub fn java_error(env: &mut JNIEnv<'_>, error: jni::errors::Error) -> String {
    let fallback = error.to_string();
    if !env.exception_check().unwrap_or(false) {
        return crate::jni_diagnostic::message(&fallback, None);
    }
    let exception = env.exception_occurred();
    let _ = env.exception_clear();
    let detail = exception
        .ok()
        .filter(|exception| !exception.is_null())
        .and_then(|exception| {
            let exception = env.auto_local(exception);
            java_text(env, exception.as_ref(), "toString").or_else(|| {
                let class = env.get_object_class(&*exception);
                let class_name = match class {
                    Ok(class) => {
                        let class = env.auto_local(class);
                        java_text(env, class.as_ref(), "getName")
                    }
                    Err(_) => {
                        let _ = env.exception_clear();
                        None
                    }
                };
                let message = java_text(env, exception.as_ref(), "getMessage");
                match (class_name, message) {
                    (Some(class), Some(message)) => Some(format!("{class}: {message}")),
                    (class, message) => class.or(message),
                }
            })
        });
    let _ = env.exception_clear();
    crate::jni_diagnostic::message(&fallback, detail.as_deref())
}

fn java_text(env: &mut JNIEnv<'_>, object: &JObject<'_>, method: &str) -> Option<String> {
    let text: jni::errors::Result<String> = env.with_local_frame(8, |env| {
        let text = JString::from(
            env.call_method(object, method, "()Ljava/lang/String;", &[])?
                .l()?,
        );
        let value = env.get_string(&text)?;
        Ok(value.into())
    });
    if text.is_err() {
        let _ = env.exception_clear();
    }
    text.ok().filter(|text| !text.is_empty())
}

/// Run a host operation and return the shared JSON envelope as a Java string.
pub fn string_result(env: &mut JNIEnv<'_>, operation: impl FnOnce() -> Result<Value>) -> jstring {
    let mut value = dispatch(operation);
    if env.exception_check().unwrap_or(false) {
        let error = java_error(env, jni::errors::Error::JavaException);
        if let Some(previous) = value["error"].as_str() {
            value["error"] = Value::String(format!("{previous}; {error}"));
        } else {
            value = error_envelope(&error);
        }
    }
    match env.new_string(value.to_string()) {
        Ok(text) => text.into_raw(),
        Err(error) => {
            let _ = java_error(env, error);
            std::ptr::null_mut()
        }
    }
}

pub fn read_string(env: &mut JNIEnv<'_>, text: &JString<'_>) -> Result<String> {
    env.get_string(text)
        .map(|s| s.into())
        .map_err(|e| java_error(env, e))
}

/// Legacy packed-buffer endpoints report every native failure as -1.
pub fn integer_result(operation: impl FnOnce() -> Result<i32>) -> jint {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
        .ok()
        .and_then(std::result::Result::ok)
        .unwrap_or(-1)
}

/// Pixel endpoints use a null Java array for both failures and panics.
pub fn bytes_result(
    env: &mut JNIEnv<'_>,
    operation: impl FnOnce() -> Result<Vec<u8>>,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    if env.exception_check().unwrap_or(false) {
        let _ = java_error(env, jni::errors::Error::JavaException);
        return std::ptr::null_mut();
    }
    match result {
        Ok(Ok(bytes)) => match env.byte_array_from_slice(&bytes) {
            Ok(array) => array.into_raw(),
            Err(error) => {
                let _ = java_error(env, error);
                std::ptr::null_mut()
            }
        },
        _ => std::ptr::null_mut(),
    }
}

/// Direct-buffer metadata captured without changing an endpoint's validation
/// order. GeometryBridge historically reports read-only buffers before lookup
/// failures, so both results are captured up front.
pub struct BufferAccess {
    pub address: Result<*mut u8>,
    pub capacity: Result<usize>,
}

impl BufferAccess {
    pub fn capture(env: &mut JNIEnv<'_>, buffer: &JByteBuffer<'_>) -> Self {
        let address = env
            .get_direct_buffer_address(buffer)
            .map_err(|error| java_error(env, error));
        let capacity = env
            .get_direct_buffer_capacity(buffer)
            .map_err(|error| java_error(env, error));
        Self { address, capacity }
    }

    /// Render-plan/bundle adapters historically validate capacity first.
    pub fn capacity_first(
        env: &mut JNIEnv<'_>,
        buffer: &JByteBuffer<'_>,
    ) -> Result<(*mut u8, usize)> {
        let capacity = env
            .get_direct_buffer_capacity(buffer)
            .map_err(|e| java_error(env, e))?;
        let address = env
            .get_direct_buffer_address(buffer)
            .map_err(|e| java_error(env, e))?;
        Ok((address, capacity))
    }

    pub fn address_first(
        env: &mut JNIEnv<'_>,
        buffer: &JByteBuffer<'_>,
    ) -> Result<(*mut u8, usize)> {
        let address = env
            .get_direct_buffer_address(buffer)
            .map_err(|e| java_error(env, e))?;
        let capacity = env
            .get_direct_buffer_capacity(buffer)
            .map_err(|e| java_error(env, e))?;
        Ok((address, capacity))
    }

    pub fn resolve(&self) -> Result<(*mut u8, usize)> {
        let address = *self.address.as_ref().map_err(Clone::clone)?;
        let capacity = *self.capacity.as_ref().map_err(Clone::clone)?;
        Ok((address, capacity))
    }
}

pub fn is_read_only(env: &mut JNIEnv<'_>, buffer: &JByteBuffer<'_>) -> Result<bool> {
    read_only_value(env, buffer.as_ref())
}

/// The diagnostic probe uses a real Java callback with this same safe lookup.
pub fn read_only_value(env: &mut JNIEnv<'_>, object: &JObject<'_>) -> Result<bool> {
    env.call_method(object, "isReadOnly", "()Z", &[])
        .and_then(|v| v.z())
        .map_err(|e| java_error(env, e))
}

/// Borrow the validated region of a Java-owned output buffer.
///
/// # Safety
/// The caller must keep its Java buffer alive, ensure the pointer is non-null,
/// writable and valid for `bytes` bytes, and prevent overlapping mutable views.
pub unsafe fn output_bytes<'a>(address: *mut u8, bytes: usize) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(address, bytes) }
}

/// # Safety
/// `address` must denote a writable region of at least `bytes.len()` bytes which
/// does not overlap the input slice, and its Java owner must remain alive.
pub unsafe fn copy_bytes(address: *mut u8, bytes: &[u8]) {
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), address, bytes.len()) }
}

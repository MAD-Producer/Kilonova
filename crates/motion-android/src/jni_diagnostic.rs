//! Bounded text for the JNI adapter's existing error channels.
const MAX_DIAGNOSTIC_CHARS: usize = 2048;

pub(crate) fn message(error: &str, throwable: Option<&str>) -> String {
    let detail = throwable.filter(|text| !text.is_empty());
    let text = match detail {
        Some(detail) => format!("{error}: {detail}"),
        None => error.to_owned(),
    };
    text.chars().take(MAX_DIAGNOSTIC_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_detail_preserves_the_original_jni_failure_and_fallback() {
        assert_eq!(
            message("JNI failure", Some("java.lang.IllegalStateException: 诊断")),
            "JNI failure: java.lang.IllegalStateException: 诊断"
        );
        assert_eq!(message("JNI failure", None), "JNI failure");
        assert_eq!(message("JNI failure", Some("")), "JNI failure");
    }

    #[test]
    fn diagnostic_limit_counts_unicode_characters_without_cutting_utf8() {
        let text = message("JNI", Some(&"错😀".repeat(2048)));
        assert_eq!(text.chars().count(), MAX_DIAGNOSTIC_CHARS);
        assert!(text.starts_with("JNI: 错😀"));
    }
}

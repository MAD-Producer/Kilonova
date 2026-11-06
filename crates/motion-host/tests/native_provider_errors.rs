use motion_host::Error;
use std::{
    error::Error as _,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[derive(Debug)]
struct Provider {
    io: std::io::Error,
    drops: Arc<AtomicUsize>,
}
impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.io)
    }
}
impl std::error::Error for Provider {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.io)
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn foreign_provider_keeps_native_source_identity_os_code_and_lifetime() {
    let directory = tempfile::tempdir().unwrap();
    let io = std::fs::File::open(directory.path().join("absent")).unwrap_err();
    let code = io.raw_os_error();
    assert!(code.is_some());
    let drops = Arc::new(AtomicUsize::new(0));
    let error = Arc::new(Error::with_context(
        "provider phase=open",
        Error::from_source(Provider {
            io,
            drops: drops.clone(),
        }),
    ));
    let Error::Context { cause, .. } = error.as_ref() else {
        panic!("missing context")
    };
    let provider = cause.source().unwrap().downcast_ref::<Provider>().unwrap();
    let pointer = provider as *const Provider;
    assert_eq!(
        provider
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        code
    );
    let clone = error.clone();
    drop(error);
    let Error::Context { cause, .. } = clone.as_ref() else {
        panic!("missing context")
    };
    assert_eq!(
        cause.source().unwrap().downcast_ref::<Provider>().unwrap() as *const Provider,
        pointer
    );
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let text = Error::from(clone.to_string());
    assert_eq!(text.to_string(), clone.to_string());
    assert!(text.source().is_none());
    drop(clone);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

fn composition(code: &str) -> Error {
    motion_core::composition::CompositionError {
        code: code.into(),
        composition: "comp-main".into(),
        message: "native primary".into(),
        details: serde_json::json!({"object":7}),
    }
    .into()
}

#[test]
fn related_failure_keeps_primary_detail_and_both_native_causes() {
    let directory = tempfile::tempdir().unwrap();
    let secondary = std::fs::File::open(directory.path().join("absent")).unwrap_err();
    let code = secondary.raw_os_error();
    let primary = Box::new(composition("source_missing"));
    let detail = primary.composition_detail().unwrap() as *const _;
    let error = Error::Related {
        context: "UI overlay window=7".into(),
        primary,
        secondary: Box::new(secondary.into()),
    };
    assert_eq!(error.composition_detail().unwrap() as *const _, detail);
    // thiserror retains the declared Box<Error>, as with Context/Cleanup.
    let native = error.source().unwrap().downcast_ref::<Box<Error>>().unwrap();
    assert_eq!(native.composition_detail().unwrap() as *const _, detail);
    assert_eq!(native.composition_detail().unwrap().code, "source_missing");
    let Error::Related { secondary, .. } = &error else {
        unreachable!()
    };
    let Error::Io(io) = secondary.as_ref() else {
        panic!("secondary lost native IO")
    };
    assert_eq!(io.raw_os_error(), code);
    assert!(error.to_string().contains("; UI overlay window=7: "));

    // A related composition failure must never promote an untyped primary.
    let error = Error::Related {
        context: "overlay".into(),
        primary: Box::new("primary text".into()),
        secondary: Box::new(composition("secondary_only")),
    };
    assert!(error.composition_detail().is_none());
    assert!(motion_host::ops::typed_error_envelope(&error)["error_detail"].is_null());
}

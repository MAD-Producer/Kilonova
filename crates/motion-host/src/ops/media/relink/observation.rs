//! Defer sink calls until the owning session has released its business lock.
use std::{
    cell::{Cell, RefCell},
    fmt::{self, Write},
};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static EVENTS: RefCell<Vec<(log::Level, String)>> = const { RefCell::new(Vec::new()) };
    static OVERFLOW: Cell<usize> = const { Cell::new(0) };
}

pub(crate) struct Scope(bool);
impl Scope {
    pub(crate) fn enter() -> Self {
        Self(ACTIVE.replace(true))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
        if self.0 {
            return;
        }
        let events = EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        for (level, text) in events {
            log::log!(level, "{text}");
        }
        let overflow = OVERFLOW.replace(0);
        if overflow > 0 {
            log::error!("asset relink observation overflow dropped={overflow} scope=session");
        }
    }
}

pub(crate) fn record(level: log::Level, args: fmt::Arguments<'_>) {
    struct Buffer(String, usize);
    impl Write for Buffer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            for ch in text.chars().take(self.1) {
                self.0.push(ch);
                self.1 -= 1;
            }
            Ok(())
        }
    }
    let mut text = Buffer(String::new(), 8192);
    let _ = text.write_fmt(args);
    if ACTIVE.get() {
        EVENTS.with(|events| {
            let mut events = events.borrow_mut();
            // One status advances one source; commit/cleanup owns at most four
            // publication directories. Bound even exceptional nested paths.
            if events.len() < 16 {
                events.push((level, text.0));
            } else {
                OVERFLOW.set(OVERFLOW.get().saturating_add(1));
            }
        });
    } else {
        log::log!(level, "{}", text.0);
    }
}

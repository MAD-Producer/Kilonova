use super::*;
use std::{
    sync::{mpsc, TryLockError},
    time::Duration,
};

fn pixels(value: u8) -> Pixels {
    Pixels {
        width: 2,
        height: 1,
        rgba: vec![value; 8],
        cached: false,
    }
}

fn other_directory(root: &Path, first: &Path) -> PathBuf {
    for index in 0..256 {
        let candidate = root
            .join(format!("other-{index}"))
            .join(format!("{:064x}.rgba", 2));
        if !std::ptr::eq(proxy_cache_mutex(first), proxy_cache_mutex(&candidate)) {
            return candidate;
        }
    }
    // On the global-lock baseline every key has the same lock. Still run the
    // real IO path so failure is a blocked read/write, not a helper assertion.
    root.join("other").join(format!("{:064x}.rgba", 2))
}

#[test]
fn unrelated_proxy_directory_io_completes_while_another_cache_is_locked() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first").join(format!("{:064x}.rgba", 1));
    let other = other_directory(root.path(), &first);
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::create_dir_all(other.parent().unwrap()).unwrap();
    write_proxy(&first, &pixels(23));
    write_proxy(&other, &pixels(41));
    let mut events = ProxyEvents::default();
    let held = proxy_cache_lock(&first, &mut events);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker_path = other.clone();
    let worker = std::thread::spawn(move || {
        entered_tx.send(()).unwrap();
        let before = read_proxy(&worker_path, 2, 1);
        write_proxy(&worker_path, &pixels(89));
        done_tx
            .send((before, read_proxy(&worker_path, 2, 1)))
            .unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let independent = done_rx.recv_timeout(Duration::from_secs(2));
    // A failing baseline must release the actual IO worker before assertions.
    drop(held);
    worker.join().unwrap();
    assert_eq!(
        independent,
        Ok((Some(vec![41; 8]), Some(vec![89; 8]))),
        "another project's proxy cache IO waited for the held directory lock"
    );
    assert_eq!(read_proxy(&first, 2, 1), Some(vec![23; 8]));
    assert_eq!(read_proxy(&other, 2, 1), Some(vec![89; 8]));
}

#[test]
fn same_directory_keys_remain_serialized_and_concurrent_writes_keep_valid_bytes() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join(format!("{:064x}.rgba", 1));
    let mut events = ProxyEvents::default();
    let held = proxy_cache_lock(&first, &mut events);
    for value in 1..=128 {
        let other = root.path().join(format!("{value:064x}.rgba"));
        assert!(matches!(
            proxy_cache_mutex(&other).try_lock(),
            Err(TryLockError::WouldBlock)
        ));
    }
    drop(held);
    let workers: Vec<_> = (1..=8)
        .map(|value| {
            let path = root.path().join(format!("{value:064x}.rgba"));
            std::thread::spawn(move || {
                for _ in 0..16 {
                    write_proxy(&path, &pixels(value));
                    assert_eq!(read_proxy(&path, 2, 1), Some(vec![value; 8]));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 8);
    for value in 1..=8 {
        let path = root.path().join(format!("{value:064x}.rgba"));
        assert_eq!(read_proxy(&path, 2, 1), Some(vec![value; 8]));
    }
}

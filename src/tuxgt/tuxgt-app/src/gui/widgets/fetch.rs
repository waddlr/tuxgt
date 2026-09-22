use std::collections::HashSet;
use std::sync::Mutex;

/// Session-scoped in-flight set. One shared art-render set keyed by game id;
/// it must stay shared (not one per kind) because a render covers all kinds.
pub(crate) struct FetchSet {
    inner: Mutex<HashSet<String>>,
}

impl FetchSet {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashSet::new()),
        }
    }

    pub(crate) fn mark(&self, id: &str) -> bool {
        self.inner.lock().expect("fetch set").insert(id.to_string())
    }

    pub(crate) fn unmark(&self, id: &str) {
        self.inner.lock().expect("fetch set").remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::FetchSet;

    #[test]
    fn mark_once_until_unmark() {
        let s = FetchSet::new();
        assert!(s.mark("a"));
        assert!(!s.mark("a"));
        s.unmark("a");
        assert!(s.mark("a"));
    }
}

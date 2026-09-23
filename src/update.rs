//! App-owned editor lifetimes for an updater's safe-shutdown request.
//!
//! This is not permission to replace files: the installer must also hold the
//! exclusive runtime lock after all application/helper processes have exited.
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
pub struct UpdateState {
    native: Cell<u32>,
    full: Cell<u32>,
    stopping: Cell<bool>,
}

#[derive(Clone, Copy)]
pub enum EditorKind {
    Native,
    Full,
}

pub struct EditorLease {
    state: Rc<UpdateState>,
    kind: EditorKind,
}

impl UpdateState {
    fn count(&self, kind: EditorKind) -> &Cell<u32> {
        match kind {
            EditorKind::Native => &self.native,
            EditorKind::Full => &self.full,
        }
    }

    pub fn track(self: &Rc<Self>, kind: EditorKind) -> Option<EditorLease> {
        if self.stopping.get() {
            return None;
        }
        let count = self.count(kind);
        count.set(count.get().checked_add(1)?);
        Some(EditorLease {
            state: self.clone(),
            kind,
        })
    }

    pub fn stopping(&self) -> bool {
        self.stopping.get()
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "protocol": 1,
            "native_editors": self.native.get(),
            "full_editors": self.full.get(),
            "quitting": self.stopping.get(),
        })
    }

    /// Runs on the GTK main thread, before acknowledging the D-Bus request.
    /// Refusal changes nothing. Acceptance gates new editor/activation requests.
    pub fn begin_shutdown(&self) -> bool {
        if self.native.get() != 0 || self.full.get() != 0 {
            return false;
        }
        self.stopping.set(true);
        true
    }
}

impl Drop for EditorLease {
    fn drop(&mut self) {
        let count = self.state.count(self.kind);
        count.set(count.get() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_native_and_full_editors_both_block_shutdown() {
        let state = Rc::new(UpdateState::default());
        let native = state.track(EditorKind::Native).unwrap();
        let full = state.track(EditorKind::Full).unwrap();
        assert_eq!(state.status()["native_editors"], 1);
        assert_eq!(state.status()["full_editors"], 1);
        assert!(!state.begin_shutdown());
        assert!(!state.stopping());
        drop(native);
        assert!(!state.begin_shutdown());
        drop(full);
        assert!(state.begin_shutdown());
        assert!(state.begin_shutdown());
        assert!(state.track(EditorKind::Native).is_none());
        assert!(state.track(EditorKind::Full).is_none());
    }

    #[test]
    fn failed_launch_releases_lease_without_poisoning_state() {
        let state = Rc::new(UpdateState::default());
        drop(state.track(EditorKind::Full).unwrap());
        assert_eq!(state.status()["full_editors"], 0);
        assert!(state.track(EditorKind::Native).is_some());
        assert!(!state.stopping());
        assert!(state.begin_shutdown());
    }
}

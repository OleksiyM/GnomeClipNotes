use gtk::gdk::{Key, ModifierType};

// The same definitions feed the GTK menu and the Settings reference.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("View", "view", "F3"),
    ("Edit", "edit", "F4"),
    ("Info", "info", "<Alt>Return"),
    ("Rename…", "rename", "F2"),
    ("Delete…", "delete", "Delete"),
];

pub fn action(key: Key, modifiers: ModifierType) -> Option<&'static str> {
    let modifiers = modifiers
        & (ModifierType::SHIFT_MASK
            | ModifierType::CONTROL_MASK
            | ModifierType::ALT_MASK
            | ModifierType::SUPER_MASK
            | ModifierType::META_MASK
            | ModifierType::HYPER_MASK);
    if modifiers == ModifierType::ALT_MASK && matches!(key, Key::Return | Key::KP_Enter) {
        return Some("info");
    }
    if !modifiers.is_empty() {
        return None;
    }
    match key {
        Key::F3 => Some("view"),
        Key::F4 => Some("edit"),
        Key::F2 => Some("rename"),
        Key::F8 | Key::Delete | Key::KP_Delete => Some("delete"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_are_exact_and_leave_system_combinations_alone() {
        for (key, modifiers, name) in [
            (Key::F3, ModifierType::empty(), "view"),
            (Key::F4, ModifierType::empty(), "edit"),
            (Key::Return, ModifierType::ALT_MASK, "info"),
            (Key::F2, ModifierType::empty(), "rename"),
            (Key::Delete, ModifierType::empty(), "delete"),
        ] {
            assert_eq!(action(key, modifiers), Some(name));
        }
        assert_eq!(action(Key::F8, ModifierType::empty()), Some("delete"));
        assert_eq!(action(Key::F1, ModifierType::empty()), None);
        assert_eq!(action(Key::F6, ModifierType::empty()), None);
        assert_eq!(action(Key::F4, ModifierType::ALT_MASK), None);
        assert_eq!(action(Key::Delete, ModifierType::CONTROL_MASK), None);
        assert_eq!(
            action(
                Key::Return,
                ModifierType::ALT_MASK | ModifierType::SHIFT_MASK
            ),
            None
        );
    }
}

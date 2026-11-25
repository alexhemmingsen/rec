//! Validating the user's global shortcut.

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Parse an accelerator like "Control+Shift+5". A shortcut needs at least one
/// modifier, so it never steals a plain key from other apps.
pub fn parse(accelerator: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|e| format!("\"{accelerator}\" is not a valid shortcut: {e}"))?;
    if shortcut.mods.is_empty() {
        return Err("Add at least one modifier key (⌃, ⌥, ⇧ or ⌘).".into());
    }
    if shortcut.mods == Modifiers::SHIFT {
        return Err("Shift alone would block typing that character. Add ⌃, ⌥ or ⌘.".into());
    }
    if shortcut.key == Code::Escape {
        return Err("Esc is used to cancel a selection.".into());
    }
    if is_modifier_key(shortcut.key) {
        return Err("Finish the shortcut with a normal key.".into());
    }
    Ok(shortcut)
}

fn is_modifier_key(code: Code) -> bool {
    matches!(
        code,
        Code::ShiftLeft
            | Code::ShiftRight
            | Code::ControlLeft
            | Code::ControlRight
            | Code::AltLeft
            | Code::AltRight
            | Code::MetaLeft
            | Code::MetaRight
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_default() {
        let s = parse(crate::settings::DEFAULT_SHORTCUT).unwrap();
        assert_eq!(s.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(s.key, Code::Digit5);
    }

    #[test]
    fn accepts_common_combinations() {
        for s in [
            "Alt+Shift+R",
            "Super+Shift+2",
            "Control+Alt+F5",
            "Control+Shift+Digit5",
        ] {
            assert!(parse(s).is_ok(), "{s}");
        }
    }

    #[test]
    fn rejects_unsafe_or_invalid_shortcuts() {
        assert!(parse("5").is_err(), "no modifier");
        assert!(parse("Shift+A").is_err(), "shift only");
        assert!(parse("Control+Escape").is_err(), "escape");
        assert!(parse("Control+Banana").is_err(), "unknown key");
        assert!(parse("").is_err());
    }
}

//! Keyboard shortcuts, focus guards, and OS reduced-motion detection.
//!
//! The resolver is a pure function so the guard rules can be unit-tested without
//! a live window. It mirrors `src/features/compose/ComposePanel.tsx`:
//! `Ctrl/Cmd+Enter` composes, `Ctrl/Cmd+Shift+Enter` (or Describe mode) describes,
//! and `Esc` cancels only while busy. Modal and text-input focus suppress the
//! shortcuts that the source suppresses.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Compose,
    Describe,
    Cancel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusContext {
    pub modal_open: bool,
    pub text_input_focused: bool,
}

/// Resolves a key event into an action, or `None` when a guard suppresses it.
pub fn resolve_shortcut(
    key: &str,
    mods: KeyModifiers,
    focus: FocusContext,
    busy: bool,
    describe_mode: bool,
) -> Option<Shortcut> {
    // A modal intercepts every shortcut, matching the source's
    // `[role="dialog"], [aria-modal="true"]` guard.
    if focus.modal_open {
        return None;
    }

    if key == "Escape" {
        return busy.then_some(Shortcut::Cancel);
    }

    if !(mods.ctrl || mods.meta) || key != "Enter" {
        return None;
    }
    // Text inputs keep Ctrl+Enter for themselves in the source.
    if focus.text_input_focused {
        return None;
    }
    if busy {
        return None;
    }
    if mods.shift || describe_mode {
        Some(Shortcut::Describe)
    } else {
        Some(Shortcut::Compose)
    }
}

/// True when Windows is configured to minimise animations. Non-Windows builds
/// return false; the theme still honours an explicit reduced-motion flag.
#[cfg(windows)]
pub fn os_prefers_reduced_motion() -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(
            ui_action: u32,
            ui_param: u32,
            pv_param: *mut i32,
            f_win_ini: u32,
        ) -> i32;
    }
    const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    let mut animations_enabled: i32 = 1;
    let ok =
        unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut animations_enabled, 0) };
    ok != 0 && animations_enabled == 0
}

#[cfg(not(windows))]
pub fn os_prefers_reduced_motion() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter() -> KeyModifiers {
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        }
    }

    #[test]
    fn compose_and_describe() {
        assert_eq!(
            resolve_shortcut("Enter", enter(), FocusContext::default(), false, false),
            Some(Shortcut::Compose)
        );
        assert_eq!(
            resolve_shortcut(
                "Enter",
                KeyModifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
                FocusContext::default(),
                false,
                false
            ),
            Some(Shortcut::Describe)
        );
        assert_eq!(
            resolve_shortcut("Enter", enter(), FocusContext::default(), false, true),
            Some(Shortcut::Describe)
        );
    }

    #[test]
    fn cancel_only_when_busy() {
        assert_eq!(
            resolve_shortcut(
                "Escape",
                KeyModifiers::default(),
                FocusContext::default(),
                true,
                false
            ),
            Some(Shortcut::Cancel)
        );
        assert_eq!(
            resolve_shortcut(
                "Escape",
                KeyModifiers::default(),
                FocusContext::default(),
                false,
                false
            ),
            None
        );
    }

    #[test]
    fn modal_suppresses_every_shortcut() {
        let focus = FocusContext {
            modal_open: true,
            text_input_focused: false,
        };
        assert_eq!(
            resolve_shortcut("Enter", enter(), focus, false, false),
            None
        );
        assert_eq!(
            resolve_shortcut("Escape", KeyModifiers::default(), focus, true, false),
            None
        );
    }

    #[test]
    fn text_input_suppresses_ctrl_enter_but_not_escape() {
        let focus = FocusContext {
            modal_open: false,
            text_input_focused: true,
        };
        assert_eq!(
            resolve_shortcut("Enter", enter(), focus, false, false),
            None
        );
        assert_eq!(
            resolve_shortcut("Escape", KeyModifiers::default(), focus, true, false),
            Some(Shortcut::Cancel)
        );
    }

    #[test]
    fn plain_enter_is_not_a_shortcut() {
        assert_eq!(
            resolve_shortcut(
                "Enter",
                KeyModifiers::default(),
                FocusContext::default(),
                false,
                false
            ),
            None
        );
    }
}

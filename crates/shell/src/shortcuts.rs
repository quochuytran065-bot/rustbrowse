//! Keyboard shortcuts and accelerator definitions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppShortcut {
    NewTab,
    CloseTab,
    Reload,
    GoBack,
    GoForward,
    FocusAddressBar,
    NextTab,
    PrevTab,
    SelectTab(usize),
    SleepBackgroundTabs,
    FreeRam,
    GamingMode,
    CommandPalette,
    ReaderMode,
    ElementZapper,
}

pub struct ShortcutRouter;

impl ShortcutRouter {
    pub fn match_shortcut(
        key_code: usize,
        is_ctrl: bool,
        is_alt: bool,
        is_shift: bool,
    ) -> Option<AppShortcut> {
        // VK_F5 = 0x74
        if key_code == 0x74 {
            return Some(AppShortcut::Reload);
        }

        if is_ctrl && !is_alt {
            if is_shift {
                match key_code {
                    0x09 => Some(AppShortcut::PrevTab),    // Ctrl+Shift+Tab
                    0x4D => Some(AppShortcut::FreeRam),    // Ctrl+Shift+M
                    0x47 => Some(AppShortcut::GamingMode), // Ctrl+Shift+G
                    _ => None,
                }
            } else {
                match key_code {
                    0x54 => Some(AppShortcut::NewTab),              // Ctrl+T
                    0x57 => Some(AppShortcut::CloseTab),            // Ctrl+W
                    0x52 => Some(AppShortcut::Reload),              // Ctrl+R
                    0x4C => Some(AppShortcut::FocusAddressBar),     // Ctrl+L
                    0x4B => Some(AppShortcut::CommandPalette),      // Ctrl+K
                    0x09 => Some(AppShortcut::NextTab),             // Ctrl+Tab
                    0x4D => Some(AppShortcut::SleepBackgroundTabs), // Ctrl+M
                    // Numbers 1 to 9 (0x31 to 0x39)
                    k @ 0x31..=0x39 => Some(AppShortcut::SelectTab(k - 0x31)),
                    _ => None,
                }
            }
        } else if is_alt && !is_ctrl {
            match key_code {
                0x25 => Some(AppShortcut::GoBack),          // Alt+Left
                0x27 => Some(AppShortcut::GoForward),       // Alt+Right
                0x44 => Some(AppShortcut::FocusAddressBar), // Alt+D
                0x52 => Some(AppShortcut::ReaderMode),      // Alt+R
                0x5A => Some(AppShortcut::ElementZapper),   // Alt+Z
                _ => None,
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcut_matching() {
        assert_eq!(
            ShortcutRouter::match_shortcut(0x54, true, false, false),
            Some(AppShortcut::NewTab)
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x57, true, false, false),
            Some(AppShortcut::CloseTab)
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x74, false, false, false),
            Some(AppShortcut::Reload)
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x09, true, false, true),
            Some(AppShortcut::PrevTab)
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x31, true, false, false),
            Some(AppShortcut::SelectTab(0))
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x47, true, false, true),
            Some(AppShortcut::GamingMode)
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x4B, true, false, false),
            Some(AppShortcut::CommandPalette)
        );
        assert_eq!(
            ShortcutRouter::match_shortcut(0x52, false, true, false),
            Some(AppShortcut::ReaderMode)
        );
    }
}

//! Named-function bindings for Linux compositor shortcuts.

use super::{DesktopKeyAction, DesktopKeyEvent};

/// A layout-resolved XKB symbol, not a physical key position.
///
/// Matching is exact, including case: use `ascii('Q')` for a shifted uppercase Q.
/// Use [`Self::from_keysym`] for non-ASCII symbols and additional named keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutKey(u32);

#[allow(non_upper_case_globals)]
impl ShortcutKey {
    pub const Space: Self = Self(0x20);
    pub const Enter: Self = Self(0xff0d);
    pub const Escape: Self = Self(0xff1b);
    pub const Tab: Self = Self(0xff09);
    pub const Backspace: Self = Self(0xff08);

    /// Creates a key from an XKB keysym value.
    pub const fn from_keysym(keysym: u32) -> Self {
        Self(keysym)
    }

    /// Creates a printable ASCII symbol.
    ///
    /// # Panics
    /// Panics if `character` is outside the printable ASCII range (space through tilde).
    pub const fn ascii(character: char) -> Self {
        assert!(
            character >= ' ' && character <= '~',
            "shortcut character must be printable ASCII"
        );
        Self(character as u32)
    }
}

/// An exact symbol and Control/Shift/Alt/Super modifier combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyChord {
    key: ShortcutKey,
    control: bool,
    shift: bool,
    alt: bool,
    logo: bool,
}

impl KeyChord {
    pub const fn new(key: ShortcutKey) -> Self {
        Self {
            key,
            control: false,
            shift: false,
            alt: false,
            logo: false,
        }
    }

    pub const fn control(mut self) -> Self {
        self.control = true;
        self
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Requires the Super/Windows modifier (the desktop event's `logo` modifier).
    pub const fn super_key(mut self) -> Self {
        self.logo = true;
        self
    }

    fn matches(self, event: DesktopKeyEvent) -> bool {
        self.key.0 == event.keysym
            && self.control == event.control
            && self.shift == event.shift
            && self.alt == event.alt
            && self.logo == event.logo
    }
}

/// A collection of compositor shortcuts backed by ordinary `fn()` pointers.
///
/// Matched keys run once per fresh press and consume that key's press, repeats, and release.
/// Unmatched keys forward normally. The desktop host disables shortcuts during session lock.
/// Functions run on the host thread and must remain short and nonblocking.
///
/// ```
/// use telorgon::app::{Compositor, KeyBindings, KeyChord, ShortcutKey};
///
/// fn open_launcher() { /* Request your application's launcher. */ }
/// fn open_terminal() { /* Request your application's terminal. */ }
///
/// let shortcuts = KeyBindings::new()
///     .bind(KeyChord::new(ShortcutKey::Space).super_key(), open_launcher)
///     .bind(KeyChord::new(ShortcutKey::Enter).super_key(), open_terminal);
/// let compositor = Compositor::new().keybindings(shortcuts);
/// ```
#[derive(Clone, Debug, Default)]
pub struct KeyBindings {
    bindings: Vec<(KeyChord, fn())>,
}

impl KeyBindings {
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Adds a shortcut backed by a named function.
    ///
    /// # Panics
    /// Panics if the exact chord is already bound, including aliases for the same keysym.
    pub fn bind(mut self, chord: KeyChord, handler: fn()) -> Self {
        assert!(
            !self.bindings.iter().any(|(existing, _)| *existing == chord),
            "duplicate compositor shortcut: {chord:?}"
        );
        self.bindings.push((chord, handler));
        self
    }

    pub(crate) fn handle(&self, event: DesktopKeyEvent) -> DesktopKeyAction {
        if let Some((_, handler)) = self.bindings.iter().find(|(chord, _)| chord.matches(event)) {
            handler();
            DesktopKeyAction::Consume
        } else {
            DesktopKeyAction::Forward
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop() {}

    #[test]
    fn matching_requires_exact_modifiers_and_symbol() {
        let bindings = KeyBindings::new().bind(
            KeyChord::new(ShortcutKey::ascii('Q'))
                .control()
                .shift()
                .alt()
                .super_key(),
            noop,
        );
        let matched = DesktopKeyEvent {
            keysym: u32::from('Q'),
            control: true,
            shift: true,
            alt: true,
            logo: true,
            ..Default::default()
        };
        assert_eq!(bindings.handle(matched), DesktopKeyAction::Consume);
        for event in [
            DesktopKeyEvent {
                control: false,
                ..matched
            },
            DesktopKeyEvent {
                shift: false,
                ..matched
            },
            DesktopKeyEvent {
                alt: false,
                ..matched
            },
            DesktopKeyEvent {
                logo: false,
                ..matched
            },
            DesktopKeyEvent {
                keysym: u32::from('q'),
                ..matched
            },
        ] {
            assert_eq!(bindings.handle(event), DesktopKeyAction::Forward);
        }
        let plain = KeyBindings::new().bind(KeyChord::new(ShortcutKey::Space), noop);
        assert_eq!(
            plain.handle(DesktopKeyEvent {
                keysym: 0x20,
                logo: true,
                ..Default::default()
            }),
            DesktopKeyAction::Forward
        );
        assert_eq!(
            KeyBindings::new().handle(matched),
            DesktopKeyAction::Forward
        );
    }

    #[test]
    #[should_panic(expected = "duplicate compositor shortcut")]
    fn aliases_cannot_register_duplicate_chords() {
        KeyBindings::new()
            .bind(KeyChord::new(ShortcutKey::Space), noop)
            .bind(KeyChord::new(ShortcutKey::ascii(' ')), noop);
    }

    #[test]
    #[should_panic(expected = "printable ASCII")]
    fn ascii_rejects_non_ascii() {
        ShortcutKey::ascii('é');
    }
}

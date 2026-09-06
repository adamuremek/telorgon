use std::collections::BTreeSet;

use crate::application_host::{DesktopKeyAction, DesktopKeyEvent};

/// Remember ownership until release, even if modifiers or focus change meanwhile.
#[derive(Default)]
pub(super) struct ShortcutKeys {
    pressed: BTreeSet<u32>,
    consumed: BTreeSet<u32>,
}

impl ShortcutKeys {
    pub(super) fn route(
        &mut self,
        event: DesktopKeyEvent,
        pressed: bool,
        locked: bool,
        handler: Option<&mut (dyn FnMut(DesktopKeyEvent) -> DesktopKeyAction + 'static)>,
    ) -> DesktopKeyAction {
        if !pressed {
            self.pressed.remove(&event.keycode);
            return if self.consumed.remove(&event.keycode) {
                DesktopKeyAction::Consume
            } else {
                DesktopKeyAction::Forward
            };
        }
        if !self.pressed.insert(event.keycode) {
            return if self.consumed.contains(&event.keycode) {
                DesktopKeyAction::Consume
            } else {
                DesktopKeyAction::Forward
            };
        }
        let action = if locked {
            DesktopKeyAction::Forward
        } else {
            handler.map_or(DesktopKeyAction::Forward, |handler| handler(event))
        };
        if action != DesktopKeyAction::Forward {
            self.consumed.insert(event.keycode);
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application_host::{KeyBindings, KeyChord, ShortcutKey};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn named_bindings_invoke_once_and_preserve_capture_and_lock_isolation() {
        thread_local! {
            static CALLS: Cell<usize> = const { Cell::new(0) };
        }
        fn launcher() {
            CALLS.with(|calls| calls.set(calls.get() + 1));
        }
        fn terminal() {
            CALLS.with(|calls| calls.set(calls.get() + 10));
        }
        CALLS.with(|calls| calls.set(0));
        let bindings = KeyBindings::new()
            .bind(KeyChord::new(ShortcutKey::Space).super_key(), launcher)
            .bind(KeyChord::new(ShortcutKey::Enter).super_key(), terminal);
        let mut handler = move |event| bindings.handle(event);
        let mut keys = ShortcutKeys::default();
        let event = DesktopKeyEvent {
            keycode: 57,
            keysym: 0x20,
            logo: true,
            ..Default::default()
        };
        assert_eq!(
            keys.route(event, true, false, Some(&mut handler)),
            DesktopKeyAction::Consume
        );
        assert_eq!(
            keys.route(event, true, false, Some(&mut handler)),
            DesktopKeyAction::Consume
        );
        // A modifier change and session lock must not leak the captured release.
        assert_eq!(
            keys.route(
                DesktopKeyEvent {
                    logo: false,
                    ..event
                },
                false,
                true,
                Some(&mut handler)
            ),
            DesktopKeyAction::Consume
        );
        assert_eq!(
            keys.route(event, true, true, Some(&mut handler)),
            DesktopKeyAction::Forward
        );
        assert_eq!(
            keys.route(event, false, true, Some(&mut handler)),
            DesktopKeyAction::Forward
        );
        CALLS.with(|calls| assert_eq!(calls.get(), 1));
        assert_eq!(
            keys.route(
                DesktopKeyEvent {
                    keycode: 28,
                    keysym: 0xff0d,
                    ..event
                },
                true,
                false,
                Some(&mut handler)
            ),
            DesktopKeyAction::Consume
        );
        CALLS.with(|calls| assert_eq!(calls.get(), 11));
    }

    #[test]
    fn consumed_press_repeat_and_release_stay_out_of_clients() {
        let count = Rc::new(Cell::new(0));
        let calls = count.clone();
        let mut handler = move |_| {
            calls.set(calls.get() + 1);
            DesktopKeyAction::Consume
        };
        let mut keys = ShortcutKeys::default();
        let mut event = DesktopKeyEvent {
            keycode: 20,
            control: true,
            ..Default::default()
        };
        assert_eq!(
            keys.route(event, true, false, Some(&mut handler)),
            DesktopKeyAction::Consume
        );
        event.control = false;
        assert_eq!(
            keys.route(event, true, false, Some(&mut handler)),
            DesktopKeyAction::Consume
        );
        assert_eq!(
            keys.route(event, false, true, Some(&mut handler)),
            DesktopKeyAction::Consume
        );
        assert_eq!(count.get(), 1);
        assert_eq!(
            keys.route(event, true, false, Some(&mut handler)),
            DesktopKeyAction::Consume
        );
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn ordinary_keys_and_locked_sessions_forward_both_edges() {
        let event = DesktopKeyEvent {
            keycode: 16,
            ..Default::default()
        };
        let mut keys = ShortcutKeys::default();
        let mut forbidden = |_| panic!("locked session must not invoke shortcuts");
        assert_eq!(
            keys.route(event, true, true, Some(&mut forbidden)),
            DesktopKeyAction::Forward
        );
        assert_eq!(
            keys.route(event, false, true, Some(&mut forbidden)),
            DesktopKeyAction::Forward
        );
        assert_eq!(
            keys.route(event, true, false, None),
            DesktopKeyAction::Forward
        );
        let mut consume = |_| DesktopKeyAction::Consume;
        assert_eq!(
            keys.route(event, true, false, Some(&mut consume)),
            DesktopKeyAction::Forward
        );
        assert_eq!(
            keys.route(event, false, false, Some(&mut consume)),
            DesktopKeyAction::Forward
        );
    }

    #[test]
    fn quit_is_returned_to_the_owner_loop() {
        let mut keys = ShortcutKeys::default();
        let mut handler = |_| DesktopKeyAction::Quit;
        assert_eq!(
            keys.route(DesktopKeyEvent::default(), true, false, Some(&mut handler)),
            DesktopKeyAction::Quit
        );
    }
}

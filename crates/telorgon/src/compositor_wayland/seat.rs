use crate::core::PointF;

use crate::compositor_wayland::{ClientId, WaylandSurfaceId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeatCapabilities {
    pub pointer: bool,
    pub keyboard: bool,
    pub touch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerFocus {
    pub client: ClientId,
    pub surface: WaylandSurfaceId,
    pub position: PointF,
    pub enter_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardFocus {
    pub client: ClientId,
    pub surface: WaylandSurfaceId,
    pub enter_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Released,
    Pressed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorImage {
    Hidden,
    ClientSurface {
        surface: WaylandSurfaceId,
        hotspot_x: i32,
        hotspot_y: i32,
    },
    Shape(u32),
    TelorgonDefault,
}

#[derive(Debug)]
pub struct SeatState {
    pub name: String,
    pub capabilities: SeatCapabilities,
    pub pointer_focus: Option<PointerFocus>,
    pub keyboard_focus: Option<KeyboardFocus>,
    pub cursor: CursorImage,
    pressed_keys: Vec<u32>,
    pressed_buttons: Vec<u32>,
    keyboard_modifiers: (u32, u32, u32, u32),
}

impl SeatState {
    pub fn new(name: impl Into<String>, capabilities: SeatCapabilities) -> Self {
        Self {
            name: name.into(),
            capabilities,
            pointer_focus: None,
            keyboard_focus: None,
            cursor: CursorImage::TelorgonDefault,
            pressed_keys: Vec::new(),
            pressed_buttons: Vec::new(),
            keyboard_modifiers: (0, 0, 0, 0),
        }
    }

    pub fn set_key(&mut self, key: u32, state: ButtonState) -> bool {
        update_pressed(&mut self.pressed_keys, key, state)
    }

    pub fn set_button(&mut self, button: u32, state: ButtonState) -> bool {
        update_pressed(&mut self.pressed_buttons, button, state)
    }

    pub fn pressed_keys(&self) -> &[u32] {
        &self.pressed_keys
    }

    pub fn pressed_buttons(&self) -> &[u32] {
        &self.pressed_buttons
    }

    pub fn keyboard_modifiers(&self) -> (u32, u32, u32, u32) {
        self.keyboard_modifiers
    }

    pub fn set_keyboard_modifiers(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) {
        self.keyboard_modifiers = (depressed, latched, locked, group);
    }

    pub fn remove_client(&mut self, client: ClientId) {
        if self
            .pointer_focus
            .is_some_and(|focus| focus.client == client)
        {
            self.pointer_focus = None;
            self.cursor = CursorImage::TelorgonDefault;
        }
        if self
            .keyboard_focus
            .is_some_and(|focus| focus.client == client)
        {
            self.keyboard_focus = None;
        }
    }

    pub fn remove_surface(&mut self, surface: WaylandSurfaceId) {
        if self
            .pointer_focus
            .is_some_and(|focus| focus.surface == surface)
        {
            self.pointer_focus = None;
            self.cursor = CursorImage::TelorgonDefault;
        }
        if self
            .keyboard_focus
            .is_some_and(|focus| focus.surface == surface)
        {
            self.keyboard_focus = None;
        }
        if matches!(
            self.cursor,
            CursorImage::ClientSurface {
                surface: cursor_surface,
                ..
            } if cursor_surface == surface
        ) {
            self.cursor = CursorImage::TelorgonDefault;
        }
    }
}

fn update_pressed(values: &mut Vec<u32>, value: u32, state: ButtonState) -> bool {
    match (
        values.iter().position(|candidate| *candidate == value),
        state,
    ) {
        (None, ButtonState::Pressed) => {
            values.push(value);
            true
        }
        (Some(index), ButtonState::Released) => {
            values.swap_remove(index);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_button_state_does_not_duplicate_pressed_state() {
        let mut seat = SeatState::new("seat0", SeatCapabilities::default());
        assert!(seat.set_button(0x110, ButtonState::Pressed));
        assert!(!seat.set_button(0x110, ButtonState::Pressed));
        assert_eq!(seat.pressed_buttons(), &[0x110]);
        assert!(seat.set_button(0x110, ButtonState::Released));
    }

    #[test]
    fn destroying_a_focused_surface_clears_focus_and_its_cursor() {
        let client = ClientId::from_raw(1).unwrap();
        let surface = WaylandSurfaceId::from_raw(7).unwrap();
        let mut seat = SeatState::new("seat0", SeatCapabilities::default());
        seat.pointer_focus = Some(PointerFocus {
            client,
            surface,
            position: PointF::default(),
            enter_serial: 1,
        });
        seat.keyboard_focus = Some(KeyboardFocus {
            client,
            surface,
            enter_serial: 2,
        });
        seat.cursor = CursorImage::ClientSurface {
            surface,
            hotspot_x: 0,
            hotspot_y: 0,
        };

        seat.remove_surface(surface);

        assert!(seat.pointer_focus.is_none());
        assert!(seat.keyboard_focus.is_none());
        assert_eq!(seat.cursor, CursorImage::TelorgonDefault);
    }

    #[test]
    fn keyboard_modifiers_are_retained_for_the_next_focus_enter() {
        let mut seat = SeatState::new("seat0", SeatCapabilities::default());
        seat.set_keyboard_modifiers(1, 2, 4, 3);

        assert_eq!(seat.keyboard_modifiers(), (1, 2, 4, 3));
    }
}

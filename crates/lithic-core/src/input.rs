use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BinaryState {
    Released,
    Pressed,
}

impl BinaryState {
    pub fn from_linux_value(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Released),
            1 => Some(Self::Pressed),
            _ => None,
        }
    }

    pub fn to_wayland(self) -> u32 {
        match self {
            Self::Released => 0,
            Self::Pressed => 1,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    PointerMotion { delta_x: i32, delta_y: i32 },
    PointerButton { button: u32, state: BinaryState },
    KeyboardKey { key: u32, state: BinaryState },
}

impl fmt::Display for InputEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PointerMotion { delta_x, delta_y } => {
                write!(f, "pointer motion dx={delta_x} dy={delta_y}")
            }
            Self::PointerButton { button, state } => {
                write!(f, "pointer button button={button} state={state:?}")
            }
            Self::KeyboardKey { key, state } => {
                write!(f, "keyboard key key={key} state={state:?}")
            }
        }
    }
}

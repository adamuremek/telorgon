use std::fmt;

use crate::core::{PointI, SizeI};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OutputTransform {
    #[default]
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputMode {
    pub size: SizeI,
    pub refresh_millihertz: u32,
    pub preferred: bool,
}

impl OutputMode {
    pub fn validate(self) -> Result<Self, OutputError> {
        if self.size.width <= 0 || self.size.height <= 0 || self.refresh_millihertz == 0 {
            return Err(OutputError::InvalidMode);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDescription {
    pub name: String,
    pub description: String,
    pub make: String,
    pub model: String,
    pub physical_millimeters: SizeI,
    pub logical_position: PointI,
    pub scale: i32,
    pub transform: OutputTransform,
    pub modes: Vec<OutputMode>,
}

impl OutputDescription {
    pub fn validate(self) -> Result<Self, OutputError> {
        if self.name.trim().is_empty()
            || self.scale <= 0
            || self.physical_millimeters.width < 0
            || self.physical_millimeters.height < 0
            || self.modes.is_empty()
            || self.modes.len() > 128
        {
            return Err(OutputError::InvalidDescription);
        }
        for mode in &self.modes {
            mode.validate()?;
        }
        if self.modes.iter().filter(|mode| mode.preferred).count() > 1 {
            return Err(OutputError::MultiplePreferredModes);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputState {
    pub description: OutputDescription,
    pub current_mode: usize,
    pub enabled: bool,
}

impl OutputState {
    pub fn new(description: OutputDescription, current_mode: usize) -> Result<Self, OutputError> {
        let description = description.validate()?;
        if current_mode >= description.modes.len() {
            return Err(OutputError::UnknownMode);
        }
        Ok(Self {
            description,
            current_mode,
            enabled: true,
        })
    }

    pub fn set_mode(&mut self, mode: usize) -> Result<(), OutputError> {
        if mode >= self.description.modes.len() {
            return Err(OutputError::UnknownMode);
        }
        self.current_mode = mode;
        Ok(())
    }

    pub fn current_mode(&self) -> OutputMode {
        self.description.modes[self.current_mode]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputError {
    InvalidMode,
    InvalidDescription,
    MultiplePreferredModes,
    UnknownMode,
}

impl fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Wayland output validation failed: {self:?}")
    }
}

impl std::error::Error for OutputError {}

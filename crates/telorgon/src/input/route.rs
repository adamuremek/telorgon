/// Whether routing continues to the next listener in the current input route.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Propagation {
    #[default]
    Continue,
    Stop,
}

/// Whether a primitive may perform its default response after listener routing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum DefaultResponse {
    #[default]
    Allow,
    Prevent,
}

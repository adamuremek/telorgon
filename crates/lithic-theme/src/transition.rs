#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeTransition {
    pub property: String,
    pub duration_ms: u32,
    pub curve: TransitionCurve,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TransitionCurve {
    Linear,
    EaseOut,
    Spring,
}

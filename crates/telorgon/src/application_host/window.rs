use crate::core::SizeI;

#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    pub title: String,
    pub size: SizeI,
    pub min_size: Option<SizeI>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Telorgon".to_owned(),
            size: SizeI {
                width: 1280,
                height: 800,
            },
            min_size: Some(SizeI {
                width: 320,
                height: 240,
            }),
        }
    }
}

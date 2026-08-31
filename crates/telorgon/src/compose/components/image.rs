use crate::ui::{BoxStyle, ImageId, LayoutStyle};

use crate::compose::{Element, ElementKind, Key, View};

#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct ImageElement {
    pub image: ImageId,
    pub content_version: u64,
    pub accessible_label: Option<String>,
    pub style: BoxStyle,
    pub layout: LayoutStyle,
}

#[derive(Clone, Debug)]
pub struct Image {
    key: Option<Key>,
    element: ImageElement,
}

impl Image {
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn content_version(mut self, content_version: u64) -> Self {
        self.element.content_version = content_version.max(1);
        self
    }

    pub fn accessible_label(mut self, label: impl Into<String>) -> Self {
        self.element.accessible_label = Some(label.into());
        self
    }

    pub fn style(mut self, style: BoxStyle) -> Self {
        self.element.style = style;
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Self {
        self.element.layout = layout;
        self
    }
}

impl View for Image {
    fn into_element(self) -> Element {
        Element::from_kind(self.key, ElementKind::Image(self.element))
    }
}

pub fn image(image: ImageId) -> Image {
    Image {
        key: None,
        element: ImageElement {
            image,
            content_version: 1,
            accessible_label: None,
            style: BoxStyle::default(),
            layout: LayoutStyle::default(),
        },
    }
}

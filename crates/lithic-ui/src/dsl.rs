use lithic_core::ColorRgba8;

use crate::{
    Action, Align, Alignment, Button, ButtonRow, ControlGroup, CrossAxisAlignment, EdgeInsetsI,
    HStack, Icon, IconButton, MainAxisAlignment, Padding, Spacer, Stack, Text, VStack, Widget,
};

pub fn widget_action(name: impl Into<String>) -> Action {
    Action::new(name)
}

pub fn stack<I, T>(children: I) -> Widget
where
    I: IntoIterator<Item = T>,
    T: Into<Widget>,
{
    Widget::Stack(Stack {
        children: children.into_iter().map(Into::into).collect(),
        alignment: Alignment::default(),
    })
}

pub fn vstack<I, T>(children: I, spacing_px: i32) -> Widget
where
    I: IntoIterator<Item = T>,
    T: Into<Widget>,
{
    Widget::VStack(VStack {
        children: children.into_iter().map(Into::into).collect(),
        spacing_px,
        main_axis_alignment: MainAxisAlignment::Start,
        cross_axis_alignment: CrossAxisAlignment::Center,
    })
}

pub fn hstack<I, T>(children: I, spacing_px: i32) -> Widget
where
    I: IntoIterator<Item = T>,
    T: Into<Widget>,
{
    Widget::HStack(HStack {
        children: children.into_iter().map(Into::into).collect(),
        spacing_px,
        main_axis_alignment: MainAxisAlignment::Start,
        cross_axis_alignment: CrossAxisAlignment::Center,
    })
}

pub fn row<I, T>(children: I, spacing_px: i32) -> Widget
where
    I: IntoIterator<Item = T>,
    T: Into<Widget>,
{
    hstack(children, spacing_px)
}

pub fn column<I, T>(children: I, spacing_px: i32) -> Widget
where
    I: IntoIterator<Item = T>,
    T: Into<Widget>,
{
    vstack(children, spacing_px)
}

pub fn align(alignment: Alignment, child: impl Into<Widget>) -> Widget {
    Widget::Align(Align {
        alignment,
        child: Box::new(child.into()),
    })
}

pub fn text(value: impl Into<String>, color: ColorRgba8) -> Widget {
    Widget::Text(Text {
        text: value.into(),
        color,
    })
}

pub fn button(
    label: impl Into<String>,
    color: ColorRgba8,
    background_color: ColorRgba8,
    action: Option<Action>,
) -> Widget {
    Widget::Button(Button {
        label: label.into(),
        color,
        background_color,
        hover_background_color: None,
        action,
    })
}

pub fn icon_button(icon: Icon, color: ColorRgba8, action: Option<Action>) -> Widget {
    Widget::IconButton(IconButton {
        icon,
        color,
        hover_background_color: None,
        action,
    })
}

pub fn button_row(accent_color: ColorRgba8, button_count: u8) -> Widget {
    Widget::ButtonRow(ButtonRow {
        accent_color,
        button_count,
    })
}

pub fn control_group<I, T>(
    children: I,
    button_size_px: i32,
    spacing_px: i32,
    margin_px: i32,
) -> Widget
where
    I: IntoIterator<Item = T>,
    T: Into<Widget>,
{
    Widget::ControlGroup(ControlGroup {
        children: children.into_iter().map(Into::into).collect(),
        button_size_px,
        spacing_px,
        margin_px,
    })
}

pub fn spacer(min_width_px: i32, min_height_px: i32) -> Widget {
    Widget::Spacer(Spacer {
        min_width_px,
        min_height_px,
    })
}

pub fn padding(insets: EdgeInsetsI, child: impl Into<Widget>) -> Widget {
    Widget::Padding(Padding {
        insets,
        child: Box::new(child.into()),
    })
}

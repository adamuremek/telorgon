extern crate self as lithic_ui;

pub use lithic_core as foundation;

pub mod dsl;

pub use dsl::{
    align, button, button_row, column, control_group, hstack, icon_button, padding, row, spacer,
    stack, text, vstack, widget_action,
};
use lithic_core::{ColorRgba8, SizeI};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetTree {
    pub root: Widget,
}

impl WidgetTree {
    pub fn new(root: Widget) -> Self {
        Self { root }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Widget {
    Stack(Stack),
    VStack(VStack),
    HStack(HStack),
    Align(Align),
    Text(Text),
    Button(Button),
    IconButton(IconButton),
    ButtonRow(ButtonRow),
    ControlGroup(ControlGroup),
    Panel(Panel),
    SplitPane(SplitPane),
    ScrollView(ScrollView),
    List(List),
    Tabs(Tabs),
    Overlay(Overlay),
    Menu(Menu),
    Tooltip(Tooltip),
    Modal(Modal),
    TextInput(TextInput),
    Slider(Slider),
    Checkbox(Checkbox),
    ImageCanvas(ImageCanvas),
    CodeEditor(CodeEditor),
    Spacer(Spacer),
    Padding(Padding),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub children: Vec<Widget>,
    pub alignment: Alignment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VStack {
    pub children: Vec<Widget>,
    pub spacing_px: i32,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HStack {
    pub children: Vec<Widget>,
    pub spacing_px: i32,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Align {
    pub alignment: Alignment,
    pub child: Box<Widget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub color: ColorRgba8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub color: ColorRgba8,
    pub background_color: ColorRgba8,
    pub hover_background_color: Option<ColorRgba8>,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconButton {
    pub icon: Icon,
    pub color: ColorRgba8,
    pub hover_background_color: Option<ColorRgba8>,
    pub action: Option<Action>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ButtonRow {
    pub accent_color: ColorRgba8,
    pub button_count: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlGroup {
    pub children: Vec<Widget>,
    pub button_size_px: i32,
    pub spacing_px: i32,
    pub margin_px: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    pub title: Option<String>,
    pub child: Box<Widget>,
    pub style: PanelStyle,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct PanelStyle {
    pub background: Option<ColorRgba8>,
    pub border_color: Option<ColorRgba8>,
    pub radius_px: i32,
    pub padding: EdgeInsetsI,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPane {
    pub axis: Axis,
    pub panes: Vec<SplitPaneItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPaneItem {
    pub id: String,
    pub child: Widget,
    pub min_size_px: i32,
    pub fraction: u16,
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollView {
    pub child: Box<Widget>,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    pub items: Vec<ListItem>,
    pub selected: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub thumbnail: Option<ImageData>,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tabs {
    pub tabs: Vec<Tab>,
    pub active: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: String,
    pub label: String,
    pub child: Widget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub base: Box<Widget>,
    pub layers: Vec<Widget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub action: Option<Action>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    pub text: String,
    pub child: Box<Widget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modal {
    pub title: String,
    pub child: Box<Widget>,
    pub close_action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextInput {
    pub id: String,
    pub value: String,
    pub placeholder: String,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slider {
    pub id: String,
    pub value: i32,
    pub min: i32,
    pub max: i32,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkbox {
    pub id: String,
    pub checked: bool,
    pub label: String,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageCanvas {
    pub id: String,
    pub image: Option<ImageData>,
    pub zoom_percent: u16,
    pub pan_x: i32,
    pub pan_y: i32,
    pub hit_regions: Vec<HitRegion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    pub size: SizeI,
    pub pixels_rgba8: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub id: String,
    pub rect: lithic_core::RectI,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeEditor {
    pub id: String,
    pub document: TextDocument,
    pub language: String,
    pub diagnostics: Vec<Diagnostic>,
    pub completions: Vec<CompletionItem>,
    pub hover: Option<Hover>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextDocument {
    pub text: String,
    pub cursor_byte: usize,
    pub selection_anchor_byte: Option<usize>,
    pub scroll_line: usize,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

impl TextDocument {
    pub fn insert(&mut self, value: &str) {
        self.replace_selection(value);
    }

    pub fn replace_selection(&mut self, value: &str) {
        let (start, end) = self.selection_range().unwrap_or((self.cursor_byte, self.cursor_byte));
        self.text.replace_range(start..end, value);
        self.cursor_byte = start + value.len();
        self.selection_anchor_byte = None;
        self.undo_depth += 1;
        self.redo_depth = 0;
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor_byte?;
        Some((anchor.min(self.cursor_byte), anchor.max(self.cursor_byte)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hover {
    pub range: TextRange,
    pub markdown: String,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct TextRange {
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Spacer {
    pub min_width_px: i32,
    pub min_height_px: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Padding {
    pub insets: EdgeInsetsI,
    pub child: Box<Widget>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Alignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl Default for Alignment {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainAxisAlignment {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for MainAxisAlignment {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    Start,
    Center,
    End,
    Stretch,
}

impl Default for CrossAxisAlignment {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeInsetsI {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl EdgeInsetsI {
    pub const fn all(value: i32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub const fn symmetric(horizontal: i32, vertical: i32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Icon {
    Close,
    ToggleExpand,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub name: String,
}

impl Action {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl From<Stack> for Widget {
    fn from(value: Stack) -> Self {
        Self::Stack(value)
    }
}

impl From<VStack> for Widget {
    fn from(value: VStack) -> Self {
        Self::VStack(value)
    }
}

impl From<HStack> for Widget {
    fn from(value: HStack) -> Self {
        Self::HStack(value)
    }
}

impl From<Align> for Widget {
    fn from(value: Align) -> Self {
        Self::Align(value)
    }
}

impl From<Text> for Widget {
    fn from(value: Text) -> Self {
        Self::Text(value)
    }
}

impl From<Button> for Widget {
    fn from(value: Button) -> Self {
        Self::Button(value)
    }
}

impl From<IconButton> for Widget {
    fn from(value: IconButton) -> Self {
        Self::IconButton(value)
    }
}

impl From<ButtonRow> for Widget {
    fn from(value: ButtonRow) -> Self {
        Self::ButtonRow(value)
    }
}

impl From<ControlGroup> for Widget {
    fn from(value: ControlGroup) -> Self {
        Self::ControlGroup(value)
    }
}

impl From<Panel> for Widget {
    fn from(value: Panel) -> Self {
        Self::Panel(value)
    }
}

impl From<SplitPane> for Widget {
    fn from(value: SplitPane) -> Self {
        Self::SplitPane(value)
    }
}

impl From<ScrollView> for Widget {
    fn from(value: ScrollView) -> Self {
        Self::ScrollView(value)
    }
}

impl From<List> for Widget {
    fn from(value: List) -> Self {
        Self::List(value)
    }
}

impl From<Tabs> for Widget {
    fn from(value: Tabs) -> Self {
        Self::Tabs(value)
    }
}

impl From<Overlay> for Widget {
    fn from(value: Overlay) -> Self {
        Self::Overlay(value)
    }
}

impl From<Menu> for Widget {
    fn from(value: Menu) -> Self {
        Self::Menu(value)
    }
}

impl From<Tooltip> for Widget {
    fn from(value: Tooltip) -> Self {
        Self::Tooltip(value)
    }
}

impl From<Modal> for Widget {
    fn from(value: Modal) -> Self {
        Self::Modal(value)
    }
}

impl From<TextInput> for Widget {
    fn from(value: TextInput) -> Self {
        Self::TextInput(value)
    }
}

impl From<Slider> for Widget {
    fn from(value: Slider) -> Self {
        Self::Slider(value)
    }
}

impl From<Checkbox> for Widget {
    fn from(value: Checkbox) -> Self {
        Self::Checkbox(value)
    }
}

impl From<ImageCanvas> for Widget {
    fn from(value: ImageCanvas) -> Self {
        Self::ImageCanvas(value)
    }
}

impl From<CodeEditor> for Widget {
    fn from(value: CodeEditor) -> Self {
        Self::CodeEditor(value)
    }
}

impl From<Spacer> for Widget {
    fn from(value: Spacer) -> Self {
        Self::Spacer(value)
    }
}

impl From<Padding> for Widget {
    fn from(value: Padding) -> Self {
        Self::Padding(value)
    }
}

#[cfg(test)]
mod tests {
    use lithic_core::ColorRgba8;

    use crate::{
        Icon, Widget, button_row, control_group, hstack, icon_button, stack, text, vstack,
        widget_action,
    };

    #[test]
    fn widgets_store_reusable_tree_data() {
        let tree = stack([
            text("Settings", ColorRgba8::rgba(1, 2, 3, 255)),
            hstack(
                [
                    button_row(ColorRgba8::rgba(4, 5, 6, 255), 3),
                    control_group(
                        [icon_button(
                            Icon::Close,
                            ColorRgba8::rgba(7, 8, 9, 255),
                            Some(widget_action("window.close")),
                        )],
                        12,
                        8,
                        10,
                    ),
                ],
                6,
            ),
        ]);

        let Widget::Stack(stack) = tree else {
            panic!("expected stack");
        };
        assert_eq!(stack.children.len(), 2);
        let Widget::HStack(row) = &stack.children[1] else {
            panic!("expected hstack");
        };
        assert_eq!(row.children.len(), 2);
    }

    #[test]
    fn vstack_is_a_first_class_layout_widget() {
        let tree = vstack([text("One", ColorRgba8::rgba(1, 1, 1, 255))], 4);
        assert!(matches!(tree, Widget::VStack(_)));
    }

    #[test]
    fn text_document_replaces_selection() {
        let mut document = crate::TextDocument {
            text: "lithic web studio".to_string(),
            cursor_byte: 10,
            selection_anchor_byte: Some(7),
            scroll_line: 0,
            undo_depth: 0,
            redo_depth: 0,
        };
        document.replace_selection("app");
        assert_eq!(document.text, "lithic app studio");
        assert_eq!(document.cursor_byte, 10);
        assert_eq!(document.undo_depth, 1);
        assert_eq!(document.redo_depth, 0);
    }
}

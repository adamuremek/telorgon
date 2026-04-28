use crate::foundation::{ColorRgba8, PointI, SizeI};
use crate::node::{
    ThemeNode, WindowControlButton, WindowControlHoverEffect, WindowControlKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowSurfaceTheme {
    pub id: String,
    pub data: WindowData,
    pub theme: SurfaceTheme,
    pub frame: SurfaceFrame,
}

impl WindowSurfaceTheme {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            data: WindowData::default(),
            theme: SurfaceTheme::default(),
            frame: SurfaceFrame::default(),
        }
    }

    pub fn data(mut self, data: WindowData) -> Self {
        self.data = data;
        self
    }

    pub fn theme(mut self, theme: SurfaceTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn frame(mut self, frame: SurfaceFrame) -> Self {
        self.frame = frame;
        self
    }

    pub fn legacy_chrome_nodes(&self) -> Vec<ThemeNode> {
        let mut nodes = vec![
            ThemeNode::Shadow {
                color: self.theme.shadow.color,
                radius_px: self.theme.shadow.blur_px,
                offset: self.theme.shadow.offset,
                strength: self.theme.shadow.strength,
            },
            ThemeNode::Border {
                color: self.theme.border.color,
                thickness_px: self.theme.border.width_px,
                radius_px: self.theme.radius_px,
            },
        ];

        if let Some(header) = self.frame.regions.iter().find(|region| region.role == FrameRegionRole::Header) {
            let mut header_children = Vec::new();
            collect_legacy_header_nodes(&header.children, &mut header_children);
            nodes.push(ThemeNode::TopRow {
                color: header.paint.color_or_transparent(),
                height_px: header.height_px.unwrap_or(0),
                children: header_children,
            });
        }

        nodes.push(ThemeNode::SurfaceContent {
            fill_color: self.theme.background.color_or_transparent(),
        });
        nodes
    }

    pub fn to_document(&self) -> String {
        let mut out = String::new();
        push_kv(&mut out, "id", &self.id);
        push_kv(
            &mut out,
            "theme.background",
            &color_to_hex(self.theme.background.color_or_transparent()),
        );
        push_kv(&mut out, "theme.border", &color_to_hex(self.theme.border.color));
        push_kv(&mut out, "theme.border_width", &self.theme.border.width_px.to_string());
        push_kv(&mut out, "theme.radius", &self.theme.radius_px.to_string());
        push_kv(&mut out, "theme.shadow", &color_to_hex(self.theme.shadow.color));
        push_kv(&mut out, "theme.shadow_x", &self.theme.shadow.offset.x.to_string());
        push_kv(&mut out, "theme.shadow_y", &self.theme.shadow.offset.y.to_string());
        push_kv(&mut out, "theme.shadow_blur", &self.theme.shadow.blur_px.to_string());
        push_kv(&mut out, "theme.shadow_strength", &self.theme.shadow.strength.to_string());
        for region in &self.frame.regions {
            out.push_str(&format!(
                "region|{}|{:?}|{}|{}|{}\n",
                escape(&region.id),
                region.role,
                region.height_px.unwrap_or(0),
                region.expands,
                color_to_hex(region.paint.color_or_transparent())
            ));
            for child in &region.children {
                write_element(&mut out, child);
            }
            out.push_str("end_region\n");
        }
        out
    }

    pub fn from_document(source: &str) -> Result<Self, String> {
        let mut theme = WindowSurfaceTheme::new("window");
        let mut current_region: Option<FrameRegion> = None;
        let mut current_slot: Option<FrameSlotBuilder> = None;
        let mut current_group: Option<ChromeButtonGroup> = None;

        for line in source.lines() {
            if line.is_empty() {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "id" => theme.id = unescape(value),
                    "theme.background" => {
                        theme.theme.background = SurfacePaint::color(parse_hex_color(value)?)
                    }
                    "theme.border" => theme.theme.border.color = parse_hex_color(value)?,
                    "theme.border_width" => theme.theme.border.width_px = parse_i32(value)?,
                    "theme.radius" => theme.theme.radius_px = parse_i32(value)?,
                    "theme.shadow" => theme.theme.shadow.color = parse_hex_color(value)?,
                    "theme.shadow_x" => theme.theme.shadow.offset.x = parse_i32(value)?,
                    "theme.shadow_y" => theme.theme.shadow.offset.y = parse_i32(value)?,
                    "theme.shadow_blur" => theme.theme.shadow.blur_px = parse_i32(value)?,
                    "theme.shadow_strength" => theme.theme.shadow.strength = parse_u8(value)?,
                    _ => {}
                }
                continue;
            }

            let parts = line.split('|').collect::<Vec<_>>();
            match parts.as_slice() {
                ["region", id, role, height, expands, paint] => {
                    current_region = Some(FrameRegion {
                        id: unescape(id),
                        role: parse_region_role(role),
                        height_px: Some(parse_i32(height)?).filter(|height| *height > 0),
                        expands: parse_bool(expands)?,
                        paint: SurfacePaint::color(parse_hex_color(paint)?),
                        layout: Some(RowLayout::default()),
                        children: Vec::new(),
                    });
                }
                ["slot", id, expanded] => {
                    current_slot = Some(FrameSlotBuilder {
                        id: unescape(id),
                        expanded: parse_bool(expanded)?,
                        child: None,
                    });
                }
                ["group", spacing] => {
                    current_group = Some(ChromeButtonGroup::new([]).spacing(parse_i32(spacing)?));
                }
                ["button", id, request, shape, width, height, radius, paint, hover, icon] => {
                    let mut button = ChromeButton::new(unescape(id), parse_request(request));
                    button.shape = parse_shape(shape);
                    button.size = SizeI {
                        width: parse_i32(width)?,
                        height: parse_i32(height)?,
                    };
                    button.radius_px = parse_i32(radius)?;
                    button.paint = parse_button_paint(paint)?;
                    button.hover_paint = parse_optional_button_paint(hover)?;
                    button.icon = parse_icon(icon);
                    if let Some(group) = &mut current_group {
                        group.buttons.push(button);
                    }
                }
                ["text", value, font_size, color] => {
                    let text = TextElement::new(parse_text_value(value)).style(TextElementStyle {
                        font_size_px: parse_i32(font_size)?,
                        weight: FontWeight::Medium,
                        color: parse_hex_color(color)?,
                    });
                    push_parsed_element(
                        &mut current_region,
                        &mut current_slot,
                        FrameElement::Text(text),
                    )?;
                }
                ["app_content"] => push_parsed_element(
                    &mut current_region,
                    &mut current_slot,
                    FrameElement::AppContent,
                )?,
                ["end_group"] => {
                    let group = current_group
                        .take()
                        .ok_or_else(|| "end_group without group".to_string())?;
                    push_parsed_element(
                        &mut current_region,
                        &mut current_slot,
                        FrameElement::ButtonGroup(group),
                    )?;
                }
                ["end_slot"] => {
                    let slot = current_slot
                        .take()
                        .ok_or_else(|| "end_slot without slot".to_string())?
                        .finish()?;
                    push_parsed_element(
                        &mut current_region,
                        &mut current_slot,
                        FrameElement::Slot(slot),
                    )?;
                }
                ["end_region"] => {
                    let mut region = current_region
                        .take()
                        .ok_or_else(|| "end_region without region".to_string())?;
                    if let Some(layout) = &mut region.layout {
                        layout.children = region.children.clone();
                    }
                    theme.frame.regions.push(region);
                }
                _ => return Err(format!("unsupported surface document line `{line}`")),
            }
        }
        Ok(theme)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowData {
    pub title: Option<String>,
    pub icon: Option<AssetRef>,
}

impl WindowData {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn icon(mut self, icon: AssetRef) -> Self {
        self.icon = Some(icon);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRef {
    pub path: String,
}

impl AssetRef {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceTheme {
    pub background: SurfacePaint,
    pub border: BorderPaint,
    pub radius_px: i32,
    pub shadow: ShadowPaint,
}

impl Default for SurfaceTheme {
    fn default() -> Self {
        Self {
            background: SurfacePaint::Color(ColorRgba8::rgba(0x18, 0x1d, 0x24, 0xff)),
            border: BorderPaint::default(),
            radius_px: 12,
            shadow: ShadowPaint::default(),
        }
    }
}

impl SurfaceTheme {
    pub fn background(mut self, paint: SurfacePaint) -> Self {
        self.background = paint;
        self
    }

    pub fn border(mut self, border: BorderPaint) -> Self {
        self.border = border;
        self
    }

    pub fn radius(mut self, radius_px: i32) -> Self {
        self.radius_px = radius_px;
        self
    }

    pub fn shadow(mut self, shadow: ShadowPaint) -> Self {
        self.shadow = shadow;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfacePaint {
    Transparent,
    Color(ColorRgba8),
}

impl SurfacePaint {
    pub fn color(color: ColorRgba8) -> Self {
        Self::Color(color)
    }

    pub fn transparent() -> Self {
        Self::Transparent
    }

    pub fn color_or_transparent(&self) -> ColorRgba8 {
        match self {
            Self::Transparent => ColorRgba8::rgba(0, 0, 0, 0),
            Self::Color(color) => *color,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorderPaint {
    pub color: ColorRgba8,
    pub width_px: i32,
}

impl Default for BorderPaint {
    fn default() -> Self {
        Self {
            color: ColorRgba8::rgba(0xd5, 0xdf, 0xec, 0xaa),
            width_px: 2,
        }
    }
}

impl BorderPaint {
    pub fn new(color: ColorRgba8, width_px: i32) -> Self {
        Self { color, width_px }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowPaint {
    pub offset: PointI,
    pub blur_px: i32,
    pub color: ColorRgba8,
    pub strength: u8,
}

impl Default for ShadowPaint {
    fn default() -> Self {
        Self {
            offset: PointI { x: 0, y: 10 },
            blur_px: 24,
            color: ColorRgba8::rgba(0, 0, 0, 0x70),
            strength: 96,
        }
    }
}

impl ShadowPaint {
    pub fn new(offset: PointI, blur_px: i32, color: ColorRgba8) -> Self {
        Self {
            offset,
            blur_px,
            color,
            strength: color.a,
        }
    }

    pub fn strength(mut self, strength: u8) -> Self {
        self.strength = strength;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceFrame {
    pub regions: Vec<FrameRegion>,
}

impl SurfaceFrame {
    pub fn new(regions: impl IntoIterator<Item = FrameRegion>) -> Self {
        Self {
            regions: regions.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameRegion {
    pub id: String,
    pub role: FrameRegionRole,
    pub height_px: Option<i32>,
    pub expands: bool,
    pub paint: SurfacePaint,
    pub layout: Option<RowLayout>,
    pub children: Vec<FrameElement>,
}

impl FrameRegion {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            role: FrameRegionRole::Custom,
            height_px: None,
            expands: false,
            paint: SurfacePaint::Transparent,
            layout: None,
            children: Vec::new(),
        }
    }

    pub fn header(mut self) -> Self {
        self.role = FrameRegionRole::Header;
        self
    }

    pub fn content(mut self) -> Self {
        self.role = FrameRegionRole::Content;
        self.expands = true;
        self
    }

    pub fn height(mut self, height_px: i32) -> Self {
        self.height_px = Some(height_px);
        self
    }

    pub fn expands(mut self) -> Self {
        self.expands = true;
        self
    }

    pub fn paint(mut self, paint: SurfacePaint) -> Self {
        self.paint = paint;
        self
    }

    pub fn layout(mut self, layout: RowLayout) -> Self {
        self.children = layout.children.clone();
        self.layout = Some(layout);
        self
    }

    pub fn child(mut self, child: FrameElement) -> Self {
        self.children.push(child);
        self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrameRegionRole {
    Header,
    Content,
    Custom,
}

impl Default for FrameRegionRole {
    fn default() -> Self {
        Self::Custom
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowLayout {
    pub padding: EdgeInsetsI,
    pub alignment: CrossAxisAlignment,
    pub children: Vec<FrameElement>,
}

impl RowLayout {
    pub fn new(children: impl IntoIterator<Item = FrameElement>) -> Self {
        Self {
            children: children.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn padding(mut self, padding: EdgeInsetsI) -> Self {
        self.padding = padding;
        self
    }

    pub fn align_center(mut self) -> Self {
        self.alignment = CrossAxisAlignment::Center;
        self
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    Start,
    #[default]
    Center,
    End,
    Stretch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameElement {
    Slot(FrameSlot),
    ButtonGroup(ChromeButtonGroup),
    Text(TextElement),
    AppContent,
}

impl From<FrameSlot> for FrameElement {
    fn from(value: FrameSlot) -> Self {
        Self::Slot(value)
    }
}

impl From<ChromeButtonGroup> for FrameElement {
    fn from(value: ChromeButtonGroup) -> Self {
        Self::ButtonGroup(value)
    }
}

impl From<TextElement> for FrameElement {
    fn from(value: TextElement) -> Self {
        Self::Text(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSlot {
    pub id: String,
    pub expanded: bool,
    pub child: Box<FrameElement>,
}

impl FrameSlot {
    pub fn new(id: impl Into<String>, child: impl Into<FrameElement>) -> Self {
        Self {
            id: id.into(),
            expanded: false,
            child: Box::new(child.into()),
        }
    }

    pub fn expanded(mut self) -> Self {
        self.expanded = true;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChromeButtonGroup {
    pub spacing_px: i32,
    pub buttons: Vec<ChromeButton>,
}

impl ChromeButtonGroup {
    pub fn new(buttons: impl IntoIterator<Item = ChromeButton>) -> Self {
        Self {
            spacing_px: 8,
            buttons: buttons.into_iter().collect(),
        }
    }

    pub fn spacing(mut self, spacing_px: i32) -> Self {
        self.spacing_px = spacing_px;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChromeButton {
    pub id: String,
    pub shape: ButtonShape,
    pub size: SizeI,
    pub radius_px: i32,
    pub paint: ButtonPaint,
    pub hover_paint: Option<ButtonPaint>,
    pub icon: Option<IconRef>,
    pub request: SurfaceRequest,
}

impl ChromeButton {
    pub fn new(id: impl Into<String>, request: SurfaceRequest) -> Self {
        Self {
            id: id.into(),
            shape: ButtonShape::RoundedRect,
            size: SizeI {
                width: 34,
                height: 30,
            },
            radius_px: 6,
            paint: ButtonPaint::Transparent,
            hover_paint: None,
            icon: None,
            request,
        }
    }

    pub fn circle(mut self, diameter_px: i32) -> Self {
        self.shape = ButtonShape::Circle;
        self.size = SizeI {
            width: diameter_px,
            height: diameter_px,
        };
        self.radius_px = diameter_px / 2;
        self
    }

    pub fn rounded_rect(mut self, size: SizeI, radius_px: i32) -> Self {
        self.shape = ButtonShape::RoundedRect;
        self.size = size;
        self.radius_px = radius_px;
        self
    }

    pub fn paint(mut self, paint: ButtonPaint) -> Self {
        self.paint = paint;
        self
    }

    pub fn hover_paint(mut self, paint: ButtonPaint) -> Self {
        self.hover_paint = Some(paint);
        self
    }

    pub fn icon(mut self, icon: Option<IconRef>) -> Self {
        self.icon = icon;
        self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ButtonShape {
    Circle,
    RoundedRect,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ButtonPaint {
    Transparent,
    Fill(ColorRgba8),
}

impl ButtonPaint {
    pub fn transparent() -> Self {
        Self::Transparent
    }

    pub fn fill(color: ColorRgba8) -> Self {
        Self::Fill(color)
    }

    fn color_or_transparent(self) -> ColorRgba8 {
        match self {
            Self::Transparent => ColorRgba8::rgba(0, 0, 0, 0),
            Self::Fill(color) => color,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconRef {
    Asset(AssetRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceRequest {
    Compositor(CompositorRequest),
    App(String),
    None,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CompositorRequest {
    Close,
    Minimize,
    ToggleExpanded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextElement {
    pub value: TextValue,
    pub alignment: TextAlignment,
    pub style: TextElementStyle,
}

impl TextElement {
    pub fn new(value: TextValue) -> Self {
        Self {
            value,
            alignment: TextAlignment::Center,
            style: TextElementStyle::default(),
        }
    }

    pub fn style(mut self, style: TextElementStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextValue {
    Literal(String),
    WindowTitle,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextAlignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextElementStyle {
    pub font_size_px: i32,
    pub weight: FontWeight,
    pub color: ColorRgba8,
}

impl Default for TextElementStyle {
    fn default() -> Self {
        Self {
            font_size_px: 13,
            weight: FontWeight::Medium,
            color: ColorRgba8::rgba(0xf3, 0xf6, 0xfb, 0xff),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FontWeight {
    Regular,
    Medium,
    Bold,
}

fn collect_legacy_header_nodes(elements: &[FrameElement], output: &mut Vec<ThemeNode>) {
    for element in elements {
        match element {
            FrameElement::Slot(slot) => {
                collect_legacy_header_nodes(std::slice::from_ref(slot.child.as_ref()), output);
            }
            FrameElement::ButtonGroup(group) => {
                output.push(ThemeNode::WindowControls {
                    buttons: group.buttons.iter().filter_map(legacy_button).collect(),
                    button_size_px: group
                        .buttons
                        .first()
                        .map(|button| button.size.height.min(button.size.width))
                        .unwrap_or(14),
                    spacing_px: group.spacing_px,
                    margin_px: 10,
                });
            }
            FrameElement::Text(text) => output.push(ThemeNode::TitleText {
                text: match &text.value {
                    TextValue::Literal(value) => value.clone(),
                    TextValue::WindowTitle => String::new(),
                },
                color: text.style.color,
            }),
            FrameElement::AppContent => {}
        }
    }
}

fn legacy_button(button: &ChromeButton) -> Option<WindowControlButton> {
    let kind = match button.request {
        SurfaceRequest::Compositor(CompositorRequest::Close) => WindowControlKind::Close,
        SurfaceRequest::Compositor(CompositorRequest::ToggleExpanded) => {
            WindowControlKind::ToggleExpand
        }
        SurfaceRequest::Compositor(CompositorRequest::Minimize)
        | SurfaceRequest::App(_)
        | SurfaceRequest::None => return None,
    };
    Some(WindowControlButton {
        kind,
        color: button.paint.color_or_transparent(),
        on_hover: button.hover_paint.map(|paint| WindowControlHoverEffect {
            background_color: paint.color_or_transparent(),
        }),
    })
}

pub fn color(value: u32) -> ColorRgba8 {
    ColorRgba8::rgba(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
        ((value >> 24) & 0xff) as u8,
    )
}

pub fn offset(x: i32, y: i32) -> PointI {
    PointI { x, y }
}

pub fn size(width: i32, height: i32) -> SizeI {
    SizeI { width, height }
}

pub fn square(value: i32) -> SizeI {
    SizeI {
        width: value,
        height: value,
    }
}

pub fn window_surface(id: impl Into<String>) -> WindowSurfaceTheme {
    WindowSurfaceTheme::new(id)
}

pub fn window_data() -> WindowData {
    WindowData::default()
}

pub fn surface_theme() -> SurfaceTheme {
    SurfaceTheme::default()
}

pub fn surface_frame(regions: impl IntoIterator<Item = FrameRegion>) -> SurfaceFrame {
    SurfaceFrame::new(regions)
}

pub fn frame_region(id: impl Into<String>) -> FrameRegion {
    FrameRegion::new(id)
}

pub fn row_layout(children: impl IntoIterator<Item = FrameElement>) -> RowLayout {
    RowLayout::new(children)
}

pub fn frame_slot(id: impl Into<String>, child: impl Into<FrameElement>) -> FrameSlot {
    FrameSlot::new(id, child)
}

pub fn chrome_button_group(buttons: impl IntoIterator<Item = ChromeButton>) -> ChromeButtonGroup {
    ChromeButtonGroup::new(buttons)
}

pub fn chrome_button(id: impl Into<String>, request: SurfaceRequest) -> ChromeButton {
    ChromeButton::new(id, request)
}

pub fn text_element(value: TextValue) -> TextElement {
    TextElement::new(value)
}

pub fn app_content() -> FrameElement {
    FrameElement::AppContent
}

struct FrameSlotBuilder {
    id: String,
    expanded: bool,
    child: Option<FrameElement>,
}

impl FrameSlotBuilder {
    fn finish(self) -> Result<FrameSlot, String> {
        let child = self
            .child
            .ok_or_else(|| format!("slot `{}` is missing a child", self.id))?;
        let mut slot = FrameSlot::new(self.id, child);
        slot.expanded = self.expanded;
        Ok(slot)
    }
}

fn push_parsed_element(
    region: &mut Option<FrameRegion>,
    slot: &mut Option<FrameSlotBuilder>,
    element: FrameElement,
) -> Result<(), String> {
    if let Some(slot) = slot {
        slot.child = Some(element);
        return Ok(());
    }
    let region = region
        .as_mut()
        .ok_or_else(|| "frame element outside region".to_string())?;
    region.children.push(element);
    Ok(())
}

fn write_element(out: &mut String, element: &FrameElement) {
    match element {
        FrameElement::Slot(slot) => {
            out.push_str(&format!("slot|{}|{}\n", escape(&slot.id), slot.expanded));
            write_element(out, slot.child.as_ref());
            out.push_str("end_slot\n");
        }
        FrameElement::ButtonGroup(group) => {
            out.push_str(&format!("group|{}\n", group.spacing_px));
            for button in &group.buttons {
                out.push_str(&format!(
                    "button|{}|{}|{:?}|{}|{}|{}|{}|{}|{}\n",
                    escape(&button.id),
                    request_to_string(&button.request),
                    button.shape,
                    button.size.width,
                    button.size.height,
                    button.radius_px,
                    button_paint_to_string(button.paint),
                    button
                        .hover_paint
                        .map(button_paint_to_string)
                        .unwrap_or_else(|| "none".to_string()),
                    icon_to_string(&button.icon),
                ));
            }
            out.push_str("end_group\n");
        }
        FrameElement::Text(text) => {
            out.push_str(&format!(
                "text|{}|{}|{}\n",
                text_value_to_string(&text.value),
                text.style.font_size_px,
                color_to_hex(text.style.color),
            ));
        }
        FrameElement::AppContent => out.push_str("app_content\n"),
    }
}

fn push_kv(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(&escape(value));
    out.push('\n');
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('|', "\\p").replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            out.push(match ch {
                'p' => '|',
                'n' => '\n',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    out
}

fn color_to_hex(color: ColorRgba8) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)
}

fn parse_hex_color(value: &str) -> Result<ColorRgba8, String> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 8 {
        return Err(format!("expected #rrggbbaa color, got `{value}`"));
    }
    let byte = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|error| error.to_string())
    };
    Ok(ColorRgba8::rgba(
        byte(0..2)?,
        byte(2..4)?,
        byte(4..6)?,
        byte(6..8)?,
    ))
}

fn parse_i32(value: &str) -> Result<i32, String> {
    value.parse::<i32>().map_err(|error| error.to_string())
}

fn parse_u8(value: &str) -> Result<u8, String> {
    value.parse::<u8>().map_err(|error| error.to_string())
}

fn parse_bool(value: &str) -> Result<bool, String> {
    value.parse::<bool>().map_err(|error| error.to_string())
}

fn parse_region_role(value: &str) -> FrameRegionRole {
    match value {
        "Header" => FrameRegionRole::Header,
        "Content" => FrameRegionRole::Content,
        _ => FrameRegionRole::Custom,
    }
}

fn request_to_string(request: &SurfaceRequest) -> String {
    match request {
        SurfaceRequest::Compositor(CompositorRequest::Close) => "compositor.close".to_string(),
        SurfaceRequest::Compositor(CompositorRequest::Minimize) => {
            "compositor.minimize".to_string()
        }
        SurfaceRequest::Compositor(CompositorRequest::ToggleExpanded) => {
            "compositor.toggle_expanded".to_string()
        }
        SurfaceRequest::App(name) => format!("app.{name}"),
        SurfaceRequest::None => "none".to_string(),
    }
}

fn parse_request(value: &str) -> SurfaceRequest {
    match value {
        "compositor.close" => SurfaceRequest::Compositor(CompositorRequest::Close),
        "compositor.minimize" => SurfaceRequest::Compositor(CompositorRequest::Minimize),
        "compositor.toggle_expanded" => {
            SurfaceRequest::Compositor(CompositorRequest::ToggleExpanded)
        }
        "none" => SurfaceRequest::None,
        value => SurfaceRequest::App(value.strip_prefix("app.").unwrap_or(value).to_string()),
    }
}

fn parse_shape(value: &str) -> ButtonShape {
    match value {
        "Circle" => ButtonShape::Circle,
        _ => ButtonShape::RoundedRect,
    }
}

fn button_paint_to_string(paint: ButtonPaint) -> String {
    match paint {
        ButtonPaint::Transparent => "transparent".to_string(),
        ButtonPaint::Fill(color) => color_to_hex(color),
    }
}

fn parse_button_paint(value: &str) -> Result<ButtonPaint, String> {
    if value == "transparent" {
        Ok(ButtonPaint::Transparent)
    } else {
        Ok(ButtonPaint::Fill(parse_hex_color(value)?))
    }
}

fn parse_optional_button_paint(value: &str) -> Result<Option<ButtonPaint>, String> {
    if value == "none" {
        Ok(None)
    } else {
        parse_button_paint(value).map(Some)
    }
}

fn icon_to_string(icon: &Option<IconRef>) -> String {
    match icon {
        Some(IconRef::Asset(asset)) => return format!("asset:{}", escape(&asset.path)),
        None => "none",
    }
    .to_string()
}

fn parse_icon(value: &str) -> Option<IconRef> {
    value
        .strip_prefix("asset:")
        .map(|path| IconRef::Asset(AssetRef::new(unescape(path))))
}

fn text_value_to_string(value: &TextValue) -> String {
    match value {
        TextValue::Literal(value) => format!("literal:{}", escape(value)),
        TextValue::WindowTitle => "window.title".to_string(),
    }
}

fn parse_text_value(value: &str) -> TextValue {
    if value == "window.title" {
        TextValue::WindowTitle
    } else {
        TextValue::Literal(unescape(value.strip_prefix("literal:").unwrap_or(value)))
    }
}

#[cfg(test)]
mod tests {
    use crate::foundation::SizeI;
    use crate::surface::{
        AssetRef, ButtonPaint, CompositorRequest, FrameElement, IconRef, SurfaceRequest,
        WindowSurfaceTheme,
    };

    use super::{
        chrome_button, chrome_button_group, frame_region, frame_slot, row_layout, surface_frame,
    };

    #[test]
    fn window_surface_theme_round_trips_asset_icon() {
        let source = WindowSurfaceTheme::new("editor").frame(surface_frame([frame_region("header")
            .header()
            .height(32)
            .layout(row_layout([FrameElement::from(frame_slot(
                "buttons",
                chrome_button_group([chrome_button(
                    "close",
                    SurfaceRequest::Compositor(CompositorRequest::Close),
                )
                .rounded_rect(SizeI { width: 32, height: 32 }, 0)
                .paint(ButtonPaint::transparent())
                .icon(Some(IconRef::Asset(AssetRef::new("icons/close.rgba"))))]),
            ))]))]));

        let parsed = WindowSurfaceTheme::from_document(&source.to_document()).unwrap();
        let button = parsed
            .frame
            .regions
            .iter()
            .flat_map(|region| &region.children)
            .find_map(|element| match element {
                FrameElement::Slot(slot) => match slot.child.as_ref() {
                    FrameElement::ButtonGroup(group) => group.buttons.first(),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();

        assert!(matches!(
            &button.icon,
            Some(IconRef::Asset(asset)) if asset.path == "icons/close.rgba"
        ));
    }
}

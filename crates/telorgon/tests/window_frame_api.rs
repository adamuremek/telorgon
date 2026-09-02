use telorgon::app::*;
use telorgon::application_host::AppRuntimeCore;
use telorgon::{AssetKey, MonotonicInstant};

const fn icon(path: &'static str) -> IconAsset {
    IconAsset::new(AssetKey::new(path))
}

const CONTROL_RESTING: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .background(Background::Color(ColorRgba8::rgba(35, 40, 52, 255)))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(225, 230, 242, 255),
};

const CONTROL_HOVERED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .background(Background::Color(ColorRgba8::rgba(60, 70, 96, 255)))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
};

const CONTROL_PRESSED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .background(Background::Color(ColorRgba8::rgba(82, 96, 132, 255)))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
};

const CONTROL_FOCUSED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .uniform_border(2.0, ColorRgba8::rgba(130, 155, 255, 255))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
};

const CONTROL_DISABLED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new().corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(120, 126, 142, 255),
};

const STANDARD_BUTTON: WindowControlButtonStyle = WindowControlButtonStyle {
    width: 38.0,
    height: 30.0,
    icon_size: 15.0,
    resting: CONTROL_RESTING,
    hovered: Some(CONTROL_HOVERED),
    pressed: Some(CONTROL_PRESSED),
    focused: Some(CONTROL_FOCUSED),
    disabled: Some(CONTROL_DISABLED),
    transition: Some(telorgon::TransitionSpec {
        duration_ms: 90,
        easing: telorgon::Easing::EaseOut,
        repeat: false,
    }),
};

const CLOSE_BUTTON: WindowControlButtonStyle = WindowControlButtonStyle {
    resting: WindowControlVisual {
        decoration: BoxDecoration::new()
            .background(Background::Color(ColorRgba8::rgba(77, 35, 45, 255)))
            .corner_radius(7.0),
        icon_tint: ColorRgba8::rgba(255, 220, 225, 255),
    },
    hovered: Some(WindowControlVisual {
        decoration: BoxDecoration::new()
            .background(Background::Color(ColorRgba8::rgba(196, 52, 72, 255)))
            .corner_radius(7.0),
        icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
    }),
    ..STANDARD_BUTTON
};

const NORMAL: WindowChromeStateStyle = WindowChromeStateStyle {
    title_bar_visible: true,
    frame_radius: 14.0,
    content_radius: 9.0,
    shadow: Some(Shadow {
        offset: PointF { x: 0.0, y: 12.0 },
        blur: 30.0,
        spread: 0.0,
        color: ColorRgba8::rgba(0, 0, 0, 128),
    }),
    resize_regions: true,
    resize_edge: 6.0,
    resize_hit_slop: Insets::all(3.0),
    content_margin: Insets::new(44.0, 6.0, 6.0, 6.0),
};

const TEST_CHROME: WindowChromeDesign = WindowChromeDesign {
    active: WindowChromePalette {
        frame_background: ColorRgba8::rgba(23, 27, 38, 255),
        frame_border: ColorRgba8::rgba(101, 119, 184, 255),
        frame_border_width: 1.0,
        title_color: ColorRgba8::rgba(245, 247, 255, 255),
        title_weight: 650,
    },
    inactive: WindowChromePalette {
        frame_background: ColorRgba8::rgba(31, 34, 43, 255),
        frame_border: ColorRgba8::rgba(65, 70, 85, 255),
        frame_border_width: 1.0,
        title_color: ColorRgba8::rgba(174, 179, 193, 255),
        title_weight: 450,
    },
    normal: NORMAL,
    maximized: WindowChromeStateStyle {
        frame_radius: 0.0,
        content_radius: 0.0,
        shadow: None,
        resize_regions: false,
        resize_edge: 0.0,
        resize_hit_slop: Insets::ZERO,
        content_margin: Insets::new(44.0, 0.0, 0.0, 0.0),
        ..NORMAL
    },
    tiled: WindowChromeStateStyle {
        frame_radius: 0.0,
        content_radius: 0.0,
        shadow: None,
        ..NORMAL
    },
    fullscreen: WindowChromeStateStyle {
        title_bar_visible: false,
        frame_radius: 0.0,
        content_radius: 0.0,
        shadow: None,
        resize_regions: false,
        resize_edge: 0.0,
        resize_hit_slop: Insets::ZERO,
        content_margin: Insets::ZERO,
    },
    title_bar: WindowTitleBarStyle {
        height: 44.0,
        padding: Insets::symmetric(7.0, 8.0),
        gap: 8.0,
        title_size: 14.0,
        app_icon_region_size: 30.0,
        app_icon_size: 20.0,
        show_client_icon: true,
        fallback_app_icon: Some(icon("icons/app.svg")),
        app_icon_opens_system_menu: true,
    },
    controls: WindowControlsDesign {
        minimize: WindowControlDesign {
            icon: icon("icons/minimize.svg"),
            style: STANDARD_BUTTON,
        },
        maximize: WindowControlDesign {
            icon: icon("icons/maximize.svg"),
            style: STANDARD_BUTTON,
        },
        restore: WindowControlDesign {
            icon: icon("icons/restore.svg"),
            style: STANDARD_BUTTON,
        },
        close: WindowControlDesign {
            icon: icon("icons/close.svg"),
            style: CLOSE_BUTTON,
        },
        gap: 6.0,
    },
    content_background: ColorRgba8::rgba(15, 18, 26, 255),
};

const PIN_WINDOW: ShellActionId = ShellActionId::named("window.pin");

#[component]
struct DesktopBackground {}

impl Component for DesktopBackground {
    fn view(&self) -> impl View {
        stack().background(ColorRgba8::rgba(10, 12, 18, 255))
    }
}

#[component]
struct Panel {}

impl Component for Panel {
    fn view(&self) -> impl View {
        row().child(text("Telorgon"))
    }
}

fn pin_window(_window: WindowChromeModel) {}

#[test]
fn easy_frame_is_a_closure_free_complete_desktop_declaration() {
    TEST_CHROME.validate().unwrap();

    let desktop = Application::desktop_environment("Telorgon")
        .compositor(
            Compositor::new()
                .window_frame(easy_window_frame(TEST_CHROME))
                .shell_action(PIN_WINDOW, pin_window)
                .background(DesktopBackground::default()),
        )
        .shell_widget(
            ShellWidget::new("Panel")
                .anchor(ShellWidgetAnchor::Top)
                .reserve_space(40.0)
                .content(Panel::default()),
        );

    assert!(format!("{desktop:?}").contains("has_compositor: true"));
}

#[test]
fn easy_frame_publishes_controls_and_all_resize_directions() {
    let component =
        easy_window_frame(TEST_CHROME).compose(WindowChromeModel::new(7, "Editor").active(true));
    let mut runtime = AppRuntimeCore::from_composed_with_extent(
        component,
        SizeI {
            width: 640,
            height: 480,
        },
    )
    .unwrap();
    runtime.prepare_frame(MonotonicInstant::ZERO, true).unwrap();
    let snapshot = telorgon::WindowChromeSnapshot::derive(runtime.ui(), runtime.layout()).unwrap();

    assert!(
        snapshot
            .regions
            .iter()
            .any(|region| { region.role == WindowChromeRole::Action(WindowAction::Close) })
    );
    assert_eq!(
        snapshot
            .regions
            .iter()
            .filter(|region| matches!(
                region.role,
                WindowChromeRole::Action(WindowAction::BeginResize(_))
            ))
            .count(),
        12
    );
    assert_eq!(snapshot.hit_test(320.0, 240.0), None);
    assert_eq!(
        snapshot.hit_test(320.0, 24.0),
        Some(WindowChromeRole::DragRegion)
    );
    assert_eq!(
        snapshot.hit_test(636.0, 4.0),
        Some(WindowChromeRole::Action(WindowAction::BeginResize(
            WindowResizeEdge::TopRight,
        )))
    );
    assert_eq!(snapshot.hit_test(630.0, 470.0), None);
    for region in snapshot.regions.iter().filter(|region| {
        matches!(
            region.role,
            WindowChromeRole::Action(WindowAction::BeginResize(_))
        )
    }) {
        match region.role {
            WindowChromeRole::Action(WindowAction::BeginResize(
                WindowResizeEdge::Top | WindowResizeEdge::Bottom,
            )) => assert_eq!(region.bounds.height, 6.0),
            WindowChromeRole::Action(WindowAction::BeginResize(
                WindowResizeEdge::Left | WindowResizeEdge::Right,
            )) => assert_eq!(region.bounds.width, 6.0),
            WindowChromeRole::Action(WindowAction::BeginResize(_)) => {
                assert!(
                    (region.bounds.width, region.bounds.height) == (12.0, 6.0)
                        || (region.bounds.width, region.bounds.height) == (6.0, 12.0)
                );
            }
            _ => unreachable!("filtered to resize regions"),
        }
    }
    let close = snapshot
        .regions
        .iter()
        .find(|region| region.role == WindowChromeRole::Action(WindowAction::Close))
        .expect("easy frame must publish its close control");
    let close_right_inset = snapshot.frame.bounds.x + snapshot.frame.bounds.width
        - (close.bounds.x + close.bounds.width);
    assert_eq!(close_right_inset, 9.0);

    let tiled = easy_window_frame(TEST_CHROME).compose(WindowChromeModel::new(8, "Tiled").tiling(
        WindowTilingState::new(
            WindowEdgeMask::TOP | WindowEdgeMask::RIGHT,
            WindowEdgeMask::TOP | WindowEdgeMask::RIGHT,
        ),
    ));
    let mut runtime = AppRuntimeCore::from_composed_with_extent(
        tiled,
        SizeI {
            width: 640,
            height: 480,
        },
    )
    .unwrap();
    runtime.prepare_frame(MonotonicInstant::ZERO, true).unwrap();
    let snapshot = telorgon::WindowChromeSnapshot::derive(runtime.ui(), runtime.layout()).unwrap();
    assert_eq!(
        snapshot
            .regions
            .iter()
            .filter(|region| matches!(
                region.role,
                WindowChromeRole::Action(WindowAction::BeginResize(_))
            ))
            .count(),
        4
    );
}

#[test]
fn easy_frame_resize_hitboxes_track_the_border_without_spilling_inward() {
    for (frame_border_width, side) in [(1.0, 2.0), (4.0, 10.0)] {
        let state = WindowChromeStateStyle {
            resize_edge: side,
            content_margin: Insets::new(44.0, side, side, side),
            ..NORMAL
        };
        let design = WindowChromeDesign {
            active: WindowChromePalette {
                frame_border_width,
                ..TEST_CHROME.active
            },
            inactive: WindowChromePalette {
                frame_border_width,
                ..TEST_CHROME.inactive
            },
            normal: state,
            tiled: state,
            ..TEST_CHROME
        };
        let capabilities = WindowChromeCapabilities {
            close: false,
            minimize: false,
            maximize: false,
            system_menu: false,
            ..WindowChromeCapabilities::MANAGED_TOPLEVEL
        };
        let component = easy_window_frame(design)
            .compose(WindowChromeModel::new(10, "Border").capabilities(capabilities));
        let mut runtime = AppRuntimeCore::from_composed_with_extent(
            component,
            SizeI {
                width: 640,
                height: 480,
            },
        )
        .unwrap();
        runtime.prepare_frame(MonotonicInstant::ZERO, true).unwrap();
        let snapshot =
            telorgon::WindowChromeSnapshot::derive(runtime.ui(), runtime.layout()).unwrap();

        let resize_region = |edge| {
            snapshot
                .regions
                .iter()
                .find(|region| {
                    region.role == WindowChromeRole::Action(WindowAction::BeginResize(edge))
                })
                .expect("configured resize edge must publish a hit region")
        };
        let top = resize_region(WindowResizeEdge::Top);
        let right = resize_region(WindowResizeEdge::Right);
        let bottom = resize_region(WindowResizeEdge::Bottom);
        let left = resize_region(WindowResizeEdge::Left);
        let frame = snapshot.frame.bounds;

        assert_eq!(top.bounds.y, frame.y + frame_border_width);
        assert_eq!(right.bounds.right(), frame.right() - frame_border_width);
        assert_eq!(bottom.bounds.bottom(), frame.bottom() - frame_border_width);
        assert_eq!(left.bounds.x, frame.x + frame_border_width);
        assert_eq!(
            top.hit_bounds.intersection(frame).unwrap().height,
            frame_border_width + side
        );
        assert_eq!(
            right.hit_bounds.intersection(frame).unwrap().width,
            frame_border_width + side
        );
        assert_eq!(
            bottom.hit_bounds.intersection(frame).unwrap().height,
            frame_border_width + side
        );
        assert_eq!(
            left.hit_bounds.intersection(frame).unwrap().width,
            frame_border_width + side
        );

        assert_eq!(
            snapshot.hit_test(320.0, top.bounds.bottom() - 0.5),
            Some(WindowChromeRole::Action(WindowAction::BeginResize(
                WindowResizeEdge::Top,
            )))
        );
        assert_eq!(
            snapshot.hit_test(320.0, top.bounds.bottom() + 0.5),
            Some(WindowChromeRole::DragRegion)
        );
        assert_eq!(
            snapshot.hit_test(right.bounds.x + 0.5, 240.0),
            Some(WindowChromeRole::Action(WindowAction::BeginResize(
                WindowResizeEdge::Right,
            )))
        );
        assert_eq!(snapshot.hit_test(right.bounds.x - 0.5, 240.0), None);

        let top_right_regions = snapshot
            .regions
            .iter()
            .filter(|region| {
                region.role
                    == WindowChromeRole::Action(WindowAction::BeginResize(
                        WindowResizeEdge::TopRight,
                    ))
            })
            .collect::<Vec<_>>();
        let horizontal_corner = top_right_regions
            .iter()
            .copied()
            .find(|region| region.bounds.height == side)
            .unwrap();
        let vertical_corner = top_right_regions
            .iter()
            .copied()
            .find(|region| region.bounds.width == side)
            .unwrap();
        assert_eq!(
            snapshot.hit_test(
                vertical_corner.bounds.x + 0.5,
                vertical_corner.bounds.y + side * 1.5,
            ),
            Some(WindowChromeRole::Action(WindowAction::BeginResize(
                WindowResizeEdge::TopRight,
            )))
        );
        assert_eq!(
            snapshot.hit_test(
                vertical_corner.bounds.x - side / 2.0,
                horizontal_corner.bounds.bottom() + side / 2.0,
            ),
            Some(WindowChromeRole::DragRegion)
        );

        for (edge, expected_x, expected_y) in [
            (
                WindowResizeEdge::TopRight,
                frame.right() - frame_border_width,
                frame.y + frame_border_width,
            ),
            (
                WindowResizeEdge::BottomRight,
                frame.right() - frame_border_width,
                frame.bottom() - frame_border_width,
            ),
            (
                WindowResizeEdge::BottomLeft,
                frame.x + frame_border_width,
                frame.bottom() - frame_border_width,
            ),
            (
                WindowResizeEdge::TopLeft,
                frame.x + frame_border_width,
                frame.y + frame_border_width,
            ),
        ] {
            for region in snapshot.regions.iter().filter(|region| {
                region.role == WindowChromeRole::Action(WindowAction::BeginResize(edge))
            }) {
                if matches!(
                    edge,
                    WindowResizeEdge::TopRight | WindowResizeEdge::BottomRight
                ) {
                    assert_eq!(region.bounds.right(), expected_x);
                } else {
                    assert_eq!(region.bounds.x, expected_x);
                }
                if matches!(
                    edge,
                    WindowResizeEdge::BottomRight | WindowResizeEdge::BottomLeft
                ) {
                    assert_eq!(region.bounds.bottom(), expected_y);
                } else {
                    assert_eq!(region.bounds.y, expected_y);
                }
            }
        }

        for region in snapshot.regions.iter().filter(|region| {
            matches!(
                region.role,
                WindowChromeRole::Action(WindowAction::BeginResize(_))
            )
        }) {
            match region.role {
                WindowChromeRole::Action(WindowAction::BeginResize(WindowResizeEdge::Top)) => {
                    assert_eq!(region.bounds.height, side);
                    assert_eq!(region.hit_bounds.bottom(), region.bounds.bottom());
                }
                WindowChromeRole::Action(WindowAction::BeginResize(WindowResizeEdge::Right)) => {
                    assert_eq!(region.bounds.width, side);
                    assert_eq!(region.hit_bounds.x, region.bounds.x);
                }
                WindowChromeRole::Action(WindowAction::BeginResize(WindowResizeEdge::Bottom)) => {
                    assert_eq!(region.bounds.height, side);
                    assert_eq!(region.hit_bounds.y, region.bounds.y);
                }
                WindowChromeRole::Action(WindowAction::BeginResize(WindowResizeEdge::Left)) => {
                    assert_eq!(region.bounds.width, side);
                    assert_eq!(region.hit_bounds.right(), region.bounds.right());
                }
                WindowChromeRole::Action(WindowAction::BeginResize(_)) => {
                    assert!(
                        (region.bounds.width, region.bounds.height) == (side * 2.0, side)
                            || (region.bounds.width, region.bounds.height) == (side, side * 2.0)
                    );
                }
                _ => unreachable!("filtered to resize regions"),
            }
        }
    }
}

#[component(no_default)]
struct AdvancedFrame {
    #[input]
    model: WindowChromeModel,
}

impl Component for AdvancedFrame {
    fn view(&self) -> impl View {
        window_frame()
            .gap(4.0)
            .child(
                row()
                    .height(42.0)
                    .child(text(&self.model.title).window_title())
                    .child(spacer())
                    .child(
                        button("Pin")
                            .window_shell_action(PIN_WINDOW)
                            .pointer_icon(telorgon::PointerIcon::Pointer),
                    )
                    .window_drag_region(),
            )
            .child(
                stack()
                    .height(5.0)
                    .window_resize(WindowResizeEdge::Top)
                    .window_hit_slop(Insets::all(4.0))
                    .window_hit_priority(500),
            )
            .content_slot(window_content_slot().margin((42.0, 5.0, 5.0, 5.0)))
    }
}

struct AdvancedFrameTemplate;

impl WindowFrameTemplate for AdvancedFrameTemplate {
    type Component = AdvancedFrame;

    fn compose(&self, model: WindowChromeModel) -> Self::Component {
        AdvancedFrame { model }
    }
}

#[test]
fn low_level_template_retains_full_composition_and_authorized_action_freedom() {
    let compositor = Compositor::new()
        .window_frame(AdvancedFrameTemplate)
        .shell_action(PIN_WINDOW, pin_window)
        .background(DesktopBackground::default());

    assert!(compositor.window_frame().is_some());
    assert!(compositor.authorizes_shell_action(PIN_WINDOW));

    let component = AdvancedFrameTemplate.compose(WindowChromeModel::new(9, "Advanced"));
    let mut runtime = AppRuntimeCore::from_composed_with_extent(
        component,
        SizeI {
            width: 500,
            height: 320,
        },
    )
    .unwrap();
    runtime.prepare_frame(MonotonicInstant::ZERO, true).unwrap();
    let snapshot = telorgon::WindowChromeSnapshot::derive(runtime.ui(), runtime.layout()).unwrap();
    assert!(
        snapshot
            .regions
            .iter()
            .any(|region| region.role == WindowChromeRole::ShellAction(PIN_WINDOW))
    );
    let resize = snapshot
        .regions
        .iter()
        .find(|region| {
            region.role
                == WindowChromeRole::Action(WindowAction::BeginResize(WindowResizeEdge::Top))
        })
        .unwrap();
    assert_eq!(resize.priority, 500);
    assert!(resize.hit_bounds.height > resize.bounds.height);
}

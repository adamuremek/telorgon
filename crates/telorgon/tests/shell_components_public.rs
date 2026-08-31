use std::mem::size_of;

use telorgon::{
    ApplicationGrid, Dock, FloatingRegion, Launcher, LockComposition, MediaStatus,
    NotificationCenter, NotificationHost, OnScreenDisplay, Panel, PanelAutoHidePolicy,
    QuickSettings, SemanticRole, ShadowFrame, ShellComponentDiagnostics, SnapPreview, StartMenu,
    StatusArea, StatusClock, StatusExtensionSlot, StatusIndicator, SystemDialog, SystemModalHost,
    Taskbar, TilingRegion, WindowControl, WindowControls, WindowFrame, WindowStack, WindowTitlebar,
    WorkspaceOverview, WorkspaceSwitcher, WorkspaceView, shell_components,
};

fn assert_prelude_paths(
    _: Option<shell_components::prelude::WindowFrame>,
    _: Option<shell_components::prelude::WindowTitlebar>,
    _: Option<shell_components::prelude::WindowControls>,
    _: Option<shell_components::prelude::ShadowFrame>,
    _: Option<shell_components::prelude::SnapPreview>,
    _: Option<shell_components::prelude::WorkspaceView>,
) {
}

fn assert_prelude_workspace_panel_paths(
    _: Option<shell_components::prelude::WindowStack>,
    _: Option<shell_components::prelude::TilingRegion>,
    _: Option<shell_components::prelude::FloatingRegion>,
    _: Option<shell_components::prelude::WorkspaceSwitcher>,
    _: Option<shell_components::prelude::WorkspaceOverview>,
    _: Option<shell_components::prelude::Panel>,
) {
}

fn assert_prelude_panel_launcher_paths(
    _: Option<shell_components::prelude::PanelAutoHidePolicy>,
    _: Option<shell_components::prelude::Taskbar>,
    _: Option<shell_components::prelude::Dock>,
    _: Option<shell_components::prelude::Launcher>,
    _: Option<shell_components::prelude::ApplicationGrid>,
    _: Option<shell_components::prelude::StartMenu>,
) {
}

fn assert_prelude_status_paths(
    _: Option<shell_components::prelude::StatusArea>,
    _: Option<shell_components::prelude::StatusClock>,
    _: Option<shell_components::prelude::StatusIndicator>,
    _: Option<shell_components::prelude::MediaStatus>,
    _: Option<shell_components::prelude::QuickSettings>,
    _: Option<shell_components::prelude::StatusExtensionSlot>,
) {
}

fn assert_prelude_notification_secure_paths(
    _: Option<shell_components::prelude::NotificationHost>,
    _: Option<shell_components::prelude::NotificationCenter>,
    _: Option<shell_components::prelude::SystemDialog>,
    _: Option<shell_components::prelude::OnScreenDisplay>,
    _: Option<shell_components::prelude::LockComposition>,
    _: Option<shell_components::prelude::SystemModalHost>,
) {
}

#[test]
fn umbrella_exports_the_initial_shell_component_catalog() {
    assert_prelude_paths(None, None, None, None, None, None);
    assert_prelude_workspace_panel_paths(None, None, None, None, None, None);
    assert_prelude_panel_launcher_paths(None, None, None, None, None, None);
    assert_prelude_status_paths(None, None, None, None, None, None);
    assert_prelude_notification_secure_paths(None, None, None, None, None, None);
    assert!(size_of::<WindowFrame>() > 0);
    assert!(size_of::<WindowTitlebar>() > 0);
    assert!(size_of::<WindowControls>() > 0);
    assert!(size_of::<ShadowFrame>() > 0);
    assert!(size_of::<SnapPreview>() > 0);
    assert!(size_of::<WorkspaceView>() > 0);
    assert!(size_of::<WindowStack>() > 0);
    assert!(size_of::<TilingRegion>() > 0);
    assert!(size_of::<FloatingRegion>() > 0);
    assert!(size_of::<WorkspaceSwitcher>() > 0);
    assert!(size_of::<WorkspaceOverview>() > 0);
    assert!(size_of::<Panel>() > 0);
    assert!(size_of::<PanelAutoHidePolicy>() > 0);
    assert!(size_of::<Taskbar>() > 0);
    assert!(size_of::<Dock>() > 0);
    assert!(size_of::<Launcher>() > 0);
    assert!(size_of::<ApplicationGrid>() > 0);
    assert!(size_of::<StartMenu>() > 0);
    assert!(size_of::<StatusArea>() > 0);
    assert!(size_of::<StatusClock>() > 0);
    assert!(size_of::<StatusIndicator>() > 0);
    assert!(size_of::<MediaStatus>() > 0);
    assert!(size_of::<QuickSettings>() > 0);
    assert!(size_of::<StatusExtensionSlot>() > 0);
    assert!(size_of::<NotificationHost>() > 0);
    assert!(size_of::<NotificationCenter>() > 0);
    assert!(size_of::<SystemDialog>() > 0);
    assert!(size_of::<OnScreenDisplay>() > 0);
    assert!(size_of::<LockComposition>() > 0);
    assert!(size_of::<SystemModalHost>() > 0);
    assert!(size_of::<ShellComponentDiagnostics>() > 0);
    assert_eq!(WindowControl::Close.label(), "Close window");
    assert!(SemanticRole::Window.requires_accessible_name());
}

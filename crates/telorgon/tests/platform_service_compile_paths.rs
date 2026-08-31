use std::num::{NonZeroU32, NonZeroU64};

use telorgon::{
    AccessibilityPublicationRequest, AccessibilityServiceKey, ClipboardClearRequest, ClipboardKind,
    ClipboardPublishRequest, ClipboardRevision, ClipboardServiceKey, ClipboardSnapshot,
    ClipboardSnapshotId, CursorAppearance, CursorAppearanceRequest, CursorSelection,
    CursorServiceKey, DataFormat, DataFormatReadRequest, DataOfferDescriptor, DataOfferId,
    DataReadMode, DataSourceKind, DataTransferServiceKey, DisplayDescriptor, DisplayId,
    DisplayLogicalBounds, DisplayProperties, DisplayRevision, DisplayServiceKey, DisplaySnapshot,
    ExternalUri, FileDialogFilter, FileDialogFilterRule, FileDialogMode, FileDialogOptions,
    FileDialogRequest, FileDialogServiceKey, FileExtension, HapticEffect, HapticIntensity,
    HapticRequest, HapticsServiceKey, MenuItemId, MenuItemState, MenuLabel, MenuPublicationRequest,
    MenuRevision, MenuScope, MenuServiceKey, MenuTree, NotificationDescriptor, NotificationId,
    NotificationPriority, NotificationPrivacy, NotificationPublicationRequest,
    NotificationRevision, NotificationServiceKey, NotificationSnapshotId, NotificationTitle,
    PhysicalExtent, PlatformMenuItem, PowerInhibitionKind, PowerInhibitionReason,
    PowerInhibitionRequest, PowerInhibitionScope, PowerServiceKey, RectF, ResolvedSemanticString,
    RestorationPublicationRequest, RestorationRecord, RestorationRevision, RestorationScope,
    RestorationServiceKey, RestorationSnapshotId, RestorationToken, SandboxAccessPolicy,
    ScaleFactor, SemanticName, SemanticNode, SemanticNodeGeometry, SemanticNodeId, SemanticRole,
    SemanticTreeGeneration, SemanticTreeNode, SemanticTreePublication, SemanticTreeRevision,
    SemanticTreeSnapshot, ServiceLookup, ServiceRegistry, SizeF, SizeHint, StandardCursor,
    StringId, TextBuffer, TextInputConfiguration, TextInputServiceKey, TextInputSession,
    TextInputSyncRequest, TextSessionId, TrustLevel, UriOpenRequest, UriServiceKey, ViewId,
    WindowAttentionIntent, WindowAttentionRequest, WindowCloseIntent, WindowCloseRequest,
    WindowServiceKey, WindowSizeConstraints, WindowSizeConstraintsRequest, WindowStateIntent,
    WindowStateRequest, WindowTitle, WindowTitleRequest,
};

#[test]
fn umbrella_exports_the_neutral_platform_service_paths() {
    let view = ViewId::from_raw(4, 2).unwrap();
    let title = WindowTitle::new("compile fixture").unwrap();
    let _title_request = WindowTitleRequest::new(view, title);
    let constraints = WindowSizeConstraints::new(
        Some(SizeF {
            width: 320.0,
            height: 200.0,
        }),
        Some(SizeF {
            width: 1_920.0,
            height: 1_080.0,
        }),
    )
    .unwrap();
    let _constraints_request = WindowSizeConstraintsRequest::new(view, constraints);
    let _state_request = WindowStateRequest::new(view, WindowStateIntent::Maximized);
    let _attention_request =
        WindowAttentionRequest::new(view, WindowAttentionIntent::Informational);
    let _close_request = WindowCloseRequest::new(view, WindowCloseIntent::ApplicationRequested);

    let format = DataFormat::mime("text/plain;charset=utf-8").unwrap();
    let offer = DataOfferDescriptor::new(
        DataOfferId::from_raw(6, 3).unwrap(),
        vec![format.clone()],
        DataSourceKind::Clipboard,
        TrustLevel::Trusted,
        vec![SizeHint::AtMost(4_096)],
    )
    .unwrap();
    let _read = DataFormatReadRequest::for_offer(
        &offer,
        format,
        NonZeroU64::new(4_096).unwrap(),
        DataReadMode::Streamed {
            max_chunk_bytes: NonZeroU32::new(1_024).unwrap(),
        },
    )
    .unwrap();

    let snapshot_id = ClipboardSnapshotId::new(ClipboardKind::System, ClipboardRevision::INITIAL);
    let _snapshot = ClipboardSnapshot::new(snapshot_id, Some(offer.clone())).unwrap();
    let _publish =
        ClipboardPublishRequest::new(ClipboardKind::System, offer, Some(snapshot_id)).unwrap();
    let _clear = ClipboardClearRequest::new(ClipboardKind::System, Some(snapshot_id)).unwrap();

    let text_session_id = TextSessionId::from_raw(2, 3).unwrap();
    let text_buffer = TextBuffer::from_text("compile fixture").unwrap();
    let mut text_session =
        TextInputSession::new(text_session_id, TextInputConfiguration::default(), 128);
    let text_open = text_session.open(&text_buffer).unwrap();
    let _text_sync = TextInputSyncRequest::new(view, text_open).unwrap();

    let root_id = SemanticNodeId::new(0, 1);
    let mut semantics = SemanticNode::new(SemanticRole::Window);
    semantics.name = SemanticName::Text(StringId(2));
    let root = SemanticTreeNode::new(
        root_id,
        None,
        vec![],
        semantics,
        SemanticNodeGeometry::view_logical(telorgon::RectF::ZERO).unwrap(),
    )
    .unwrap();
    let tree = SemanticTreeSnapshot::new(
        SemanticTreeGeneration::INITIAL,
        SemanticTreeRevision::INITIAL,
        root_id,
        vec![root],
        vec![ResolvedSemanticString::new(StringId(2), "compile tree").unwrap()],
        None,
        None,
    )
    .unwrap();
    let _accessibility =
        AccessibilityPublicationRequest::new(view, SemanticTreePublication::Activate(tree));
    let _cursor = CursorAppearanceRequest::new(
        view,
        CursorAppearance::new(CursorSelection::Standard(StandardCursor::Pointer), true),
    );
    let display = DisplayId::from_raw(3, 1).unwrap();
    let display_descriptor = DisplayDescriptor::new(
        display,
        DisplayLogicalBounds::new(RectF {
            x: 0.0,
            y: 0.0,
            width: 1_920.0,
            height: 1_080.0,
        })
        .unwrap(),
        PhysicalExtent::new(1_920, 1_080),
        ScaleFactor::default(),
        DisplayProperties::default(),
    )
    .unwrap();
    let _displays = DisplaySnapshot::new(
        DisplayRevision::INITIAL,
        vec![display_descriptor],
        Some(display),
    )
    .unwrap();
    let _uri = UriOpenRequest::new(
        view,
        ExternalUri::new("https://example.com/compile-fixture").unwrap(),
    );
    let file_filter = FileDialogFilter::new(
        "Images",
        vec![FileDialogFilterRule::Extension(
            FileExtension::new("png").unwrap(),
        )],
    )
    .unwrap();
    let file_options = FileDialogOptions::new(
        FileDialogMode::OpenFile,
        vec![file_filter],
        None,
        std::num::NonZeroU16::new(2).unwrap(),
        SandboxAccessPolicy::PlatformDefault,
    )
    .unwrap();
    let _file_dialog = FileDialogRequest::new(view, file_options);
    let menu_item = PlatformMenuItem::action(
        MenuItemId::from_raw(1).unwrap(),
        MenuLabel::new("Compile item").unwrap(),
        None,
        MenuItemState::default(),
        None,
    )
    .unwrap();
    let menu = MenuTree::new(
        MenuScope::View(view),
        MenuRevision::INITIAL,
        vec![menu_item],
    )
    .unwrap();
    let _menu_publication = MenuPublicationRequest::initial(menu).unwrap();
    let notification = NotificationDescriptor::new(
        NotificationSnapshotId::new(
            NotificationId::from_raw(1).unwrap(),
            NotificationRevision::INITIAL,
        ),
        NotificationTitle::new("Compile notification").unwrap(),
        None,
        NotificationPriority::Normal,
        NotificationPrivacy::Public,
        vec![],
    )
    .unwrap();
    let _notification_publication = NotificationPublicationRequest::initial(notification).unwrap();
    let _haptic = HapticRequest::new(HapticEffect::Selection, HapticIntensity::FULL);
    let _power = PowerInhibitionRequest::new(
        PowerInhibitionScope::Application,
        PowerInhibitionKind::Idle,
        PowerInhibitionReason::InteractiveActivity,
    );
    let _restoration = RestorationPublicationRequest::initial(RestorationRecord::new(
        RestorationSnapshotId::new(RestorationScope::Application, RestorationRevision::INITIAL),
        RestorationToken::new(vec![1, 2, 3]).unwrap(),
    ))
    .unwrap();

    let registry = ServiceRegistry::new();
    assert!(matches!(
        registry.lookup::<WindowServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<DataTransferServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<ClipboardServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<TextInputServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<AccessibilityServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<CursorServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<DisplayServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<UriServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<FileDialogServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<MenuServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<NotificationServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<HapticsServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<PowerServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
    assert!(matches!(
        registry.lookup::<RestorationServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));
}

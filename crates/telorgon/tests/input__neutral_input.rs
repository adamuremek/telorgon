use telorgon::core::{PointF, SizeF};
use telorgon::input::{
    ActivationInput, ActivationStateMachine, ActivationTransition, ActiveShortcutScope,
    ButtonState, ChangeSource, CompositeChange, CompositeItem, CompositeNavigationCommand,
    CompositeNavigationPolicy, CompositeSelectionBehavior, CompositeSelectionRequest,
    CompositeStateMachine, DefaultResponse, EventPhase, FocusCandidate, FocusIndicatorPolicy,
    FocusOrigin, FocusScopeId, FocusStateMachine, FocusTraversalDirection, FocusTraversalEdge,
    GestureArena, GestureArenaDecision, GestureArenaLossReason, GestureArenaRequest,
    GestureArenaWinReason, GestureInput, GestureTransition, InputEvent, KeyEvent, KeyLocation,
    KeyText, LogicalKey, Modifiers, NamedKey, PhysicalKey, PhysicalKeyCode,
    PhysicalPointerPosition, PhysicalScrollDelta, PointerButton, PointerButtonSet,
    PointerCaptureRequest, PointerContactGeometry, PointerDeviceId, PointerDeviceKind,
    PointerEvent, PointerEventKind, PointerEventSource, PointerId, PointerInputEvent,
    PointerPosition, PointerPressure, PointerProperties, PointerStateSnapshot, Propagation,
    ScrollDelta, ScrollEvent, ScrollMomentumPhase, ScrollPhase, ScrollPrecision, ScrollUnit,
    ShortcutBinding, ShortcutChord, ShortcutMatcher, ShortcutResolution, ShortcutScopeId,
    TapRecognizer, WritingDirection,
};

#[test]
fn pointer_values_preserve_contact_and_tool_identity() {
    let event = InputEvent::PointerButton {
        pointer: PointerId::new(7),
        device: PointerDeviceKind::Pen,
        button: PointerButton::new(9),
        state: ButtonState::Pressed,
    };

    assert_eq!(
        event,
        InputEvent::PointerButton {
            pointer: PointerId::new(7),
            device: PointerDeviceKind::Pen,
            button: PointerButton::new(9),
            state: ButtonState::Pressed,
        }
    );
}

#[test]
fn complete_pointer_and_scroll_values_are_public_and_source_neutral() {
    let device_id = PointerDeviceId::from_raw(3, 2).unwrap();
    let physical_position = PhysicalPointerPosition::new(30.0, 45.0).unwrap();
    let position =
        PointerPosition::with_physical(PointF { x: 15.0, y: 22.5 }, physical_position).unwrap();
    let buttons = PointerButtonSet::new([PointerButton::PRIMARY]).unwrap();
    let state = PointerStateSnapshot::new(PointerId::new(8), PointerDeviceKind::Pen)
        .with_device_id(Some(device_id))
        .with_position(Some(position))
        .with_buttons(buttons)
        .with_properties(PointerProperties {
            pressure: Some(PointerPressure::new(0.75).unwrap()),
            contact_geometry: Some(
                PointerContactGeometry::new(SizeF {
                    width: 4.0,
                    height: 6.0,
                })
                .unwrap(),
            ),
            ..PointerProperties::default()
        })
        .with_primary_contact(true)
        .with_source(PointerEventSource::Native)
        .with_modifiers(Modifiers::SHIFT);
    let pointer = PointerInputEvent::from(
        PointerEvent::new(
            PointerEventKind::Button {
                button: PointerButton::PRIMARY,
                state: ButtonState::Pressed,
            },
            state,
        )
        .unwrap(),
    );
    let PointerInputEvent::Pointer(pointer) = pointer else {
        panic!("expected complete pointer event");
    };
    assert_eq!(pointer.state().device_id(), Some(device_id));
    assert_eq!(pointer.state().position(), Some(position));
    assert!(pointer.state().primary_contact());
    assert!(pointer.state().modifiers().contains(Modifiers::SHIFT));

    let physical_delta = PhysicalScrollDelta::new(0.0, -12.0).unwrap();
    let delta = ScrollDelta::new(0.0, -6.0, ScrollUnit::Pixels)
        .unwrap()
        .with_physical_pixels(physical_delta)
        .unwrap();
    let scroll = PointerInputEvent::from(
        ScrollEvent::new(PointerId::PRIMARY, PointerDeviceKind::Mouse, delta)
            .with_device_id(Some(device_id))
            .with_position(Some(position))
            .with_phase(ScrollPhase::Changed)
            .with_momentum(ScrollMomentumPhase::Began)
            .with_precision(ScrollPrecision::Precise),
    );
    let PointerInputEvent::Scroll(scroll) = scroll else {
        panic!("expected complete scroll event");
    };
    assert_eq!(scroll.delta().physical_pixels(), Some(physical_delta));
    assert_eq!(scroll.phase(), ScrollPhase::Changed);
    assert_eq!(scroll.momentum(), ScrollMomentumPhase::Began);
    assert_eq!(scroll.precision(), ScrollPrecision::Precise);
}

#[test]
fn keyboard_values_are_native_type_free() {
    let event = InputEvent::Key(KeyEvent {
        physical_key: PhysicalKey::from_code(PhysicalKeyCode::NumpadEnter),
        logical_key: LogicalKey::Named(NamedKey::Enter),
        text: Some(KeyText::new("private").unwrap()),
        location: KeyLocation::Numpad,
        state: ButtonState::Released,
        repeat: true,
        synthetic: true,
        modifiers: Modifiers::SHIFT.union(Modifiers::CONTROL),
    });

    let InputEvent::Key(key) = event else {
        panic!("expected a key event");
    };
    assert!(key.repeat);
    assert!(key.synthetic);
    assert_eq!(key.location, KeyLocation::Numpad);
    assert_eq!(key.logical_key, LogicalKey::Named(NamedKey::Enter));
    assert!(key.modifiers.contains(Modifiers::SHIFT));
    assert!(key.modifiers.contains(Modifiers::CONTROL));
    let debug = format!("{key:?}");
    assert!(debug.contains("redacted"));
    assert!(!debug.contains("private"));
}

#[test]
fn public_shortcut_path_returns_a_typed_command_without_executing_it() {
    let scope = ShortcutScopeId::from_raw(1, 1).unwrap();
    let chord = ShortcutChord::pressed(PhysicalKey::new(42), Modifiers::CONTROL);
    let mut matcher = ShortcutMatcher::<u32, u32>::new();
    matcher
        .update_bindings([ShortcutBinding::new(7, 99, scope, chord)])
        .unwrap();

    assert_eq!(
        matcher
            .resolve(
                KeyEvent {
                    physical_key: PhysicalKey::new(42),
                    state: ButtonState::Pressed,
                    repeat: false,
                    modifiers: Modifiers::CONTROL,
                    ..KeyEvent::new(PhysicalKey::new(42), ButtonState::Pressed)
                },
                [ActiveShortcutScope::bubble(scope)],
            )
            .unwrap(),
        ShortcutResolution::Matched {
            binding: 7,
            command: 99,
            scope,
            chord,
        }
    );
}

#[test]
fn mouse_constructors_use_the_primary_mouse_identity() {
    assert_eq!(
        InputEvent::mouse_moved(PointF { x: 2.0, y: 3.0 }),
        InputEvent::PointerMoved {
            pointer: PointerId::PRIMARY,
            device: PointerDeviceKind::Mouse,
            position: PointF { x: 2.0, y: 3.0 },
        }
    );
}

#[test]
fn routing_decisions_are_orthogonal_values() {
    assert_eq!(EventPhase::Capture, EventPhase::Capture);
    assert_eq!(Propagation::default(), Propagation::Continue);
    assert_eq!(DefaultResponse::default(), DefaultResponse::Allow);
    assert_ne!(Propagation::Stop, Propagation::Continue);
    assert_ne!(DefaultResponse::Prevent, DefaultResponse::Allow);
}

#[test]
fn public_activation_path_is_source_neutral() {
    let pointer = PointerId::new(9);
    let mut activation = ActivationStateMachine::new(true);
    let down = activation.handle(ActivationInput::PointerDown {
        pointer,
        button: PointerButton::PRIMARY,
    });
    assert_eq!(down.capture, PointerCaptureRequest::Capture(pointer));
    assert_eq!(
        activation
            .handle(ActivationInput::PointerUp {
                pointer,
                button: PointerButton::PRIMARY,
                inside: true,
            })
            .transition,
        ActivationTransition::Activated(telorgon::input::Activation {
            source: ChangeSource::Pointer,
        })
    );
}

#[test]
fn public_focus_path_consumes_canonical_identity_order() {
    let root = FocusScopeId::from_raw(1, 1).unwrap();
    let mut focus = FocusStateMachine::new(
        root,
        FocusTraversalEdge::Stop,
        FocusIndicatorPolicy::Automatic,
    );
    focus
        .update_candidates(vec![
            FocusCandidate::new((1_u32, 1_u32), root),
            FocusCandidate::new((2_u32, 1_u32), root),
        ])
        .unwrap();
    focus.traverse(FocusTraversalDirection::Forward);
    focus.request_focus((2, 1), FocusOrigin::Keyboard).unwrap();
    assert_eq!(focus.focused(), Some((2, 1)));
    assert!(focus.focus_visible());
}

#[test]
fn public_composite_path_emits_controlled_keyed_selection() {
    let mut composite = CompositeStateMachine::new(CompositeNavigationPolicy {
        selection: CompositeSelectionBehavior::FollowsHighlight,
        ..CompositeNavigationPolicy::default()
    });
    composite
        .update_items([
            CompositeItem {
                key: (1_u32, 1_u32),
                enabled: true,
            },
            CompositeItem {
                key: (2_u32, 1_u32),
                enabled: true,
            },
        ])
        .unwrap();
    composite.enter(Some((1, 1))).unwrap();
    assert!(matches!(
        composite
            .navigate(
                CompositeNavigationCommand::Next,
                WritingDirection::LeftToRight,
            )
            .unwrap(),
        CompositeChange::Highlighted {
            current: (2, 1),
            selection_request: Some(CompositeSelectionRequest {
                key: (2, 1),
                source: ChangeSource::Directional,
            }),
            ..
        }
    ));
}

#[test]
fn public_gesture_path_resolves_a_tap_through_the_arena() {
    let pointer = PointerId::new(11);
    let position = PointF { x: 4.0, y: 7.0 };
    let mut arena = GestureArena::new();
    arena.add(pointer, "tap").unwrap();
    arena.add(pointer, "drag").unwrap();
    arena.close(pointer).unwrap();

    let mut tap = TapRecognizer::new(8.0, true).unwrap();
    tap.handle(GestureInput::PointerDown {
        pointer,
        button: PointerButton::PRIMARY,
        position,
    })
    .unwrap();
    let claim = tap
        .handle(GestureInput::PointerUp {
            pointer,
            button: PointerButton::PRIMARY,
            position,
        })
        .unwrap();
    assert_eq!(claim.arena, GestureArenaRequest::Accept(pointer));
    assert_eq!(
        arena.accept(pointer, "tap").unwrap(),
        vec![
            GestureArenaDecision::Lost {
                participant: "drag",
                reason: GestureArenaLossReason::Winner("tap"),
            },
            GestureArenaDecision::Won {
                participant: "tap",
                reason: GestureArenaWinReason::Accepted,
            },
        ]
    );
    assert!(matches!(
        tap.handle(GestureInput::ArenaWon { pointer })
            .unwrap()
            .transition,
        GestureTransition::TapRecognized { .. }
    ));
}

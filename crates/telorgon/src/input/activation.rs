use crate::input::{PointerButton, PointerId};

/// The source of a requested semantic value change.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChangeSource {
    Pointer,
    Keyboard,
    Directional,
    Accessibility,
    Programmatic,
}

/// A completed baseline activation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Activation {
    pub source: ChangeSource,
}

/// A competing pointer gesture that owns the interaction instead of normal activation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CompetingGesture {
    LongPress,
    DoublePress,
    ContextMenu,
    Drag,
}

/// Why an in-progress activation returned to idle without producing an action.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActivationCancelReason {
    ReleasedOutside,
    PointerCancelled,
    CaptureLost,
    GestureClaimed(CompetingGesture),
    FocusLost,
    ViewDeactivated,
    Disabled,
    Unmounted,
}

/// Current state of the source-neutral activation transition engine.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActivationPhase {
    Idle,
    PointerArmed { pointer: PointerId, inside: bool },
    KeyboardArmed,
    Dead,
}

impl ActivationPhase {
    pub const fn is_armed(self) -> bool {
        matches!(self, Self::PointerArmed { .. } | Self::KeyboardArmed)
    }

    pub const fn is_visually_armed(self) -> bool {
        matches!(
            self,
            Self::PointerArmed { inside: true, .. } | Self::KeyboardArmed
        )
    }
}

/// Input intents accepted after low-level routing and eligibility checks.
///
/// These values deliberately name semantic Space/Enter behavior rather than native key codes. Gate
/// 9 platform adapters own native and logical-key translation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActivationInput {
    PointerDown {
        pointer: PointerId,
        button: PointerButton,
    },
    PointerMoved {
        pointer: PointerId,
        inside: bool,
    },
    PointerUp {
        pointer: PointerId,
        button: PointerButton,
        inside: bool,
    },
    PointerCancelled {
        pointer: PointerId,
    },
    PointerCaptureLost {
        pointer: PointerId,
    },
    PointerGestureClaimed {
        pointer: PointerId,
        gesture: CompetingGesture,
    },
    SpaceDown {
        repeat: bool,
    },
    SpaceUp,
    EnterDown {
        repeat: bool,
    },
    SemanticActivate,
    FocusLost,
    ViewDeactivated,
    SetEnabled(bool),
    Unmount,
}

/// Pointer-capture work the runtime must perform after a transition.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum PointerCaptureRequest {
    #[default]
    None,
    Capture(PointerId),
    Release(PointerId),
}

/// Observable result of one activation transition.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActivationTransition {
    #[default]
    Ignored,
    Armed {
        source: ChangeSource,
    },
    VisualArmedChanged {
        armed: bool,
    },
    Activated(Activation),
    Cancelled {
        source: ChangeSource,
        reason: ActivationCancelReason,
    },
    EnabledChanged {
        enabled: bool,
    },
    Dead,
}

/// Complete result of one input, including any capture handoff.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ActivationOutcome {
    pub transition: ActivationTransition,
    pub capture: PointerCaptureRequest,
}

impl ActivationOutcome {
    const fn ignored() -> Self {
        Self {
            transition: ActivationTransition::Ignored,
            capture: PointerCaptureRequest::None,
        }
    }

    const fn transition(transition: ActivationTransition) -> Self {
        Self {
            transition,
            capture: PointerCaptureRequest::None,
        }
    }
}

/// Neutral baseline arm/activate/cancel transition engine.
///
/// The engine owns transient behavior state only. Typed component actions, focus/capture ownership,
/// gesture recognition, semantics, paint state, and platform conversion remain with their named
/// owners. Callers apply `PointerCaptureRequest` after the surrounding input transaction accepts
/// the default behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationStateMachine {
    enabled: bool,
    phase: ActivationPhase,
}

impl ActivationStateMachine {
    pub const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            phase: ActivationPhase::Idle,
        }
    }

    pub const fn enabled(&self) -> bool {
        self.enabled && !matches!(self.phase, ActivationPhase::Dead)
    }

    pub const fn phase(&self) -> ActivationPhase {
        self.phase
    }

    pub const fn is_armed(&self) -> bool {
        self.phase.is_armed()
    }

    pub const fn is_visually_armed(&self) -> bool {
        self.phase.is_visually_armed()
    }

    pub fn handle(&mut self, input: ActivationInput) -> ActivationOutcome {
        if matches!(self.phase, ActivationPhase::Dead) {
            return ActivationOutcome::ignored();
        }

        match input {
            ActivationInput::SetEnabled(enabled) => self.set_enabled(enabled),
            ActivationInput::Unmount => self.unmount(),
            ActivationInput::ViewDeactivated => {
                self.cancel(ActivationCancelReason::ViewDeactivated, false, false)
            }
            ActivationInput::FocusLost => {
                if matches!(self.phase, ActivationPhase::KeyboardArmed) {
                    self.cancel(ActivationCancelReason::FocusLost, false, false)
                } else {
                    ActivationOutcome::ignored()
                }
            }
            _ if !self.enabled => ActivationOutcome::ignored(),
            ActivationInput::PointerDown { pointer, button } => self.pointer_down(pointer, button),
            ActivationInput::PointerMoved { pointer, inside } => {
                self.pointer_moved(pointer, inside)
            }
            ActivationInput::PointerUp {
                pointer,
                button,
                inside,
            } => self.pointer_up(pointer, button, inside),
            ActivationInput::PointerCancelled { pointer } => {
                self.cancel_pointer(pointer, ActivationCancelReason::PointerCancelled, false)
            }
            ActivationInput::PointerCaptureLost { pointer } => {
                self.cancel_pointer(pointer, ActivationCancelReason::CaptureLost, true)
            }
            ActivationInput::PointerGestureClaimed { pointer, gesture } => self.cancel_pointer(
                pointer,
                ActivationCancelReason::GestureClaimed(gesture),
                false,
            ),
            ActivationInput::SpaceDown { repeat } => self.space_down(repeat),
            ActivationInput::SpaceUp => self.space_up(),
            ActivationInput::EnterDown { repeat } => self.enter_down(repeat),
            ActivationInput::SemanticActivate => self.semantic_activate(),
        }
    }

    fn set_enabled(&mut self, enabled: bool) -> ActivationOutcome {
        if self.enabled == enabled {
            return ActivationOutcome::ignored();
        }
        self.enabled = enabled;
        if !enabled && self.phase.is_armed() {
            self.cancel(ActivationCancelReason::Disabled, false, false)
        } else {
            ActivationOutcome::transition(ActivationTransition::EnabledChanged { enabled })
        }
    }

    fn unmount(&mut self) -> ActivationOutcome {
        if self.phase.is_armed() {
            self.cancel(ActivationCancelReason::Unmounted, true, false)
        } else {
            self.phase = ActivationPhase::Dead;
            ActivationOutcome::transition(ActivationTransition::Dead)
        }
    }

    fn pointer_down(&mut self, pointer: PointerId, button: PointerButton) -> ActivationOutcome {
        if button != PointerButton::PRIMARY || !matches!(self.phase, ActivationPhase::Idle) {
            return ActivationOutcome::ignored();
        }
        self.phase = ActivationPhase::PointerArmed {
            pointer,
            inside: true,
        };
        ActivationOutcome {
            transition: ActivationTransition::Armed {
                source: ChangeSource::Pointer,
            },
            capture: PointerCaptureRequest::Capture(pointer),
        }
    }

    fn pointer_moved(&mut self, pointer: PointerId, inside: bool) -> ActivationOutcome {
        let ActivationPhase::PointerArmed {
            pointer: owner,
            inside: was_inside,
        } = self.phase
        else {
            return ActivationOutcome::ignored();
        };
        if owner != pointer || was_inside == inside {
            return ActivationOutcome::ignored();
        }
        self.phase = ActivationPhase::PointerArmed { pointer, inside };
        ActivationOutcome::transition(ActivationTransition::VisualArmedChanged { armed: inside })
    }

    fn pointer_up(
        &mut self,
        pointer: PointerId,
        button: PointerButton,
        inside: bool,
    ) -> ActivationOutcome {
        let ActivationPhase::PointerArmed { pointer: owner, .. } = self.phase else {
            return ActivationOutcome::ignored();
        };
        if owner != pointer || button != PointerButton::PRIMARY {
            return ActivationOutcome::ignored();
        }
        self.phase = ActivationPhase::Idle;
        let transition = if inside {
            ActivationTransition::Activated(Activation {
                source: ChangeSource::Pointer,
            })
        } else {
            ActivationTransition::Cancelled {
                source: ChangeSource::Pointer,
                reason: ActivationCancelReason::ReleasedOutside,
            }
        };
        ActivationOutcome {
            transition,
            capture: PointerCaptureRequest::Release(pointer),
        }
    }

    fn cancel_pointer(
        &mut self,
        pointer: PointerId,
        reason: ActivationCancelReason,
        capture_already_lost: bool,
    ) -> ActivationOutcome {
        let ActivationPhase::PointerArmed { pointer: owner, .. } = self.phase else {
            return ActivationOutcome::ignored();
        };
        if owner != pointer {
            return ActivationOutcome::ignored();
        }
        self.cancel(reason, false, capture_already_lost)
    }

    fn space_down(&mut self, repeat: bool) -> ActivationOutcome {
        if repeat || !matches!(self.phase, ActivationPhase::Idle) {
            return ActivationOutcome::ignored();
        }
        self.phase = ActivationPhase::KeyboardArmed;
        ActivationOutcome::transition(ActivationTransition::Armed {
            source: ChangeSource::Keyboard,
        })
    }

    fn space_up(&mut self) -> ActivationOutcome {
        if !matches!(self.phase, ActivationPhase::KeyboardArmed) {
            return ActivationOutcome::ignored();
        }
        self.phase = ActivationPhase::Idle;
        ActivationOutcome::transition(ActivationTransition::Activated(Activation {
            source: ChangeSource::Keyboard,
        }))
    }

    fn enter_down(&mut self, repeat: bool) -> ActivationOutcome {
        if repeat || !matches!(self.phase, ActivationPhase::Idle) {
            return ActivationOutcome::ignored();
        }
        ActivationOutcome::transition(ActivationTransition::Activated(Activation {
            source: ChangeSource::Keyboard,
        }))
    }

    fn semantic_activate(&mut self) -> ActivationOutcome {
        if !matches!(self.phase, ActivationPhase::Idle) {
            return ActivationOutcome::ignored();
        }
        ActivationOutcome::transition(ActivationTransition::Activated(Activation {
            source: ChangeSource::Accessibility,
        }))
    }

    fn cancel(
        &mut self,
        reason: ActivationCancelReason,
        terminal: bool,
        capture_already_lost: bool,
    ) -> ActivationOutcome {
        let previous = self.phase;
        let (source, pointer) = match previous {
            ActivationPhase::PointerArmed { pointer, .. } => (ChangeSource::Pointer, Some(pointer)),
            ActivationPhase::KeyboardArmed => (ChangeSource::Keyboard, None),
            ActivationPhase::Idle | ActivationPhase::Dead => return ActivationOutcome::ignored(),
        };
        self.phase = if terminal {
            ActivationPhase::Dead
        } else {
            ActivationPhase::Idle
        };
        ActivationOutcome {
            transition: ActivationTransition::Cancelled { source, reason },
            capture: if capture_already_lost {
                PointerCaptureRequest::None
            } else {
                pointer.map_or(PointerCaptureRequest::None, PointerCaptureRequest::Release)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINTER: PointerId = PointerId::new(7);

    fn pointer_down(machine: &mut ActivationStateMachine) -> ActivationOutcome {
        machine.handle(ActivationInput::PointerDown {
            pointer: POINTER,
            button: PointerButton::PRIMARY,
        })
    }

    #[test]
    fn pointer_capture_survives_visual_disarm_and_rearm_then_releases_once() {
        let mut machine = ActivationStateMachine::new(true);
        assert_eq!(
            pointer_down(&mut machine),
            ActivationOutcome {
                transition: ActivationTransition::Armed {
                    source: ChangeSource::Pointer,
                },
                capture: PointerCaptureRequest::Capture(POINTER),
            }
        );

        assert_eq!(
            machine.handle(ActivationInput::PointerMoved {
                pointer: POINTER,
                inside: false,
            }),
            ActivationOutcome::transition(ActivationTransition::VisualArmedChanged {
                armed: false,
            })
        );
        assert!(machine.is_armed());
        assert!(!machine.is_visually_armed());
        assert_eq!(
            machine.handle(ActivationInput::PointerMoved {
                pointer: POINTER,
                inside: true,
            }),
            ActivationOutcome::transition(ActivationTransition::VisualArmedChanged { armed: true })
        );

        assert_eq!(
            machine.handle(ActivationInput::PointerUp {
                pointer: POINTER,
                button: PointerButton::PRIMARY,
                inside: true,
            }),
            ActivationOutcome {
                transition: ActivationTransition::Activated(Activation {
                    source: ChangeSource::Pointer,
                }),
                capture: PointerCaptureRequest::Release(POINTER),
            }
        );
        assert_eq!(machine.phase(), ActivationPhase::Idle);
    }

    #[test]
    fn release_outside_cancels_instead_of_activating() {
        let mut machine = ActivationStateMachine::new(true);
        pointer_down(&mut machine);
        assert_eq!(
            machine.handle(ActivationInput::PointerUp {
                pointer: POINTER,
                button: PointerButton::PRIMARY,
                inside: false,
            }),
            ActivationOutcome {
                transition: ActivationTransition::Cancelled {
                    source: ChangeSource::Pointer,
                    reason: ActivationCancelReason::ReleasedOutside,
                },
                capture: PointerCaptureRequest::Release(POINTER),
            }
        );
    }

    #[test]
    fn secondary_and_mismatched_pointers_cannot_complete_primary_activation() {
        let mut machine = ActivationStateMachine::new(true);
        assert_eq!(
            machine.handle(ActivationInput::PointerDown {
                pointer: POINTER,
                button: PointerButton::SECONDARY,
            }),
            ActivationOutcome::ignored()
        );
        pointer_down(&mut machine);
        assert_eq!(
            machine.handle(ActivationInput::PointerUp {
                pointer: PointerId::new(8),
                button: PointerButton::PRIMARY,
                inside: true,
            }),
            ActivationOutcome::ignored()
        );
        assert!(machine.is_armed());
    }

    #[test]
    fn pointer_cancel_and_capture_loss_have_distinct_capture_handoffs() {
        let mut cancelled = ActivationStateMachine::new(true);
        pointer_down(&mut cancelled);
        assert_eq!(
            cancelled.handle(ActivationInput::PointerCancelled { pointer: POINTER }),
            ActivationOutcome {
                transition: ActivationTransition::Cancelled {
                    source: ChangeSource::Pointer,
                    reason: ActivationCancelReason::PointerCancelled,
                },
                capture: PointerCaptureRequest::Release(POINTER),
            }
        );

        let mut lost = ActivationStateMachine::new(true);
        pointer_down(&mut lost);
        assert_eq!(
            lost.handle(ActivationInput::PointerCaptureLost { pointer: POINTER }),
            ActivationOutcome {
                transition: ActivationTransition::Cancelled {
                    source: ChangeSource::Pointer,
                    reason: ActivationCancelReason::CaptureLost,
                },
                capture: PointerCaptureRequest::None,
            }
        );
    }

    #[test]
    fn disabling_during_press_cancels_and_disabled_inputs_are_ignored() {
        let mut machine = ActivationStateMachine::new(true);
        pointer_down(&mut machine);
        assert_eq!(
            machine.handle(ActivationInput::SetEnabled(false)),
            ActivationOutcome {
                transition: ActivationTransition::Cancelled {
                    source: ChangeSource::Pointer,
                    reason: ActivationCancelReason::Disabled,
                },
                capture: PointerCaptureRequest::Release(POINTER),
            }
        );
        assert!(!machine.enabled());
        assert_eq!(pointer_down(&mut machine), ActivationOutcome::ignored());
        assert_eq!(
            machine.handle(ActivationInput::SetEnabled(true)),
            ActivationOutcome::transition(ActivationTransition::EnabledChanged { enabled: true })
        );
    }

    #[test]
    fn unmount_is_terminal_and_releases_pointer_capture() {
        let mut machine = ActivationStateMachine::new(true);
        pointer_down(&mut machine);
        assert_eq!(
            machine.handle(ActivationInput::Unmount),
            ActivationOutcome {
                transition: ActivationTransition::Cancelled {
                    source: ChangeSource::Pointer,
                    reason: ActivationCancelReason::Unmounted,
                },
                capture: PointerCaptureRequest::Release(POINTER),
            }
        );
        assert_eq!(machine.phase(), ActivationPhase::Dead);
        assert_eq!(pointer_down(&mut machine), ActivationOutcome::ignored());
    }

    #[test]
    fn view_deactivation_cancels_any_armed_source() {
        let mut pointer = ActivationStateMachine::new(true);
        pointer_down(&mut pointer);
        assert!(matches!(
            pointer.handle(ActivationInput::ViewDeactivated).transition,
            ActivationTransition::Cancelled {
                reason: ActivationCancelReason::ViewDeactivated,
                ..
            }
        ));

        let mut keyboard = ActivationStateMachine::new(true);
        keyboard.handle(ActivationInput::SpaceDown { repeat: false });
        assert!(matches!(
            keyboard.handle(ActivationInput::ViewDeactivated).transition,
            ActivationTransition::Cancelled {
                source: ChangeSource::Keyboard,
                reason: ActivationCancelReason::ViewDeactivated,
            }
        ));
    }

    #[test]
    fn space_arms_once_and_activates_only_after_a_matching_press() {
        let mut machine = ActivationStateMachine::new(true);
        assert_eq!(
            machine.handle(ActivationInput::SpaceUp),
            ActivationOutcome::ignored()
        );
        assert!(matches!(
            machine
                .handle(ActivationInput::SpaceDown { repeat: false })
                .transition,
            ActivationTransition::Armed {
                source: ChangeSource::Keyboard
            }
        ));
        assert_eq!(
            machine.handle(ActivationInput::SpaceDown { repeat: true }),
            ActivationOutcome::ignored()
        );
        assert_eq!(
            machine.handle(ActivationInput::SpaceUp).transition,
            ActivationTransition::Activated(Activation {
                source: ChangeSource::Keyboard,
            })
        );
    }

    #[test]
    fn focus_loss_cancels_keyboard_arming_but_not_pointer_capture() {
        let mut keyboard = ActivationStateMachine::new(true);
        keyboard.handle(ActivationInput::SpaceDown { repeat: false });
        assert!(matches!(
            keyboard.handle(ActivationInput::FocusLost).transition,
            ActivationTransition::Cancelled {
                reason: ActivationCancelReason::FocusLost,
                ..
            }
        ));

        let mut pointer = ActivationStateMachine::new(true);
        pointer_down(&mut pointer);
        assert_eq!(
            pointer.handle(ActivationInput::FocusLost),
            ActivationOutcome::ignored()
        );
        assert!(pointer.is_armed());
    }

    #[test]
    fn enter_is_immediate_and_repeats_do_not_duplicate_activation() {
        let mut machine = ActivationStateMachine::new(true);
        assert_eq!(
            machine
                .handle(ActivationInput::EnterDown { repeat: false })
                .transition,
            ActivationTransition::Activated(Activation {
                source: ChangeSource::Keyboard,
            })
        );
        assert_eq!(
            machine.handle(ActivationInput::EnterDown { repeat: true }),
            ActivationOutcome::ignored()
        );
    }

    #[test]
    fn semantic_activation_is_immediate_and_accessibility_sourced() {
        let mut machine = ActivationStateMachine::new(true);
        assert_eq!(
            machine.handle(ActivationInput::SemanticActivate).transition,
            ActivationTransition::Activated(Activation {
                source: ChangeSource::Accessibility,
            })
        );
    }

    #[test]
    fn competing_gestures_suppress_the_later_normal_pointer_up() {
        for gesture in [
            CompetingGesture::LongPress,
            CompetingGesture::DoublePress,
            CompetingGesture::ContextMenu,
            CompetingGesture::Drag,
        ] {
            let mut machine = ActivationStateMachine::new(true);
            pointer_down(&mut machine);
            assert_eq!(
                machine
                    .handle(ActivationInput::PointerGestureClaimed {
                        pointer: POINTER,
                        gesture,
                    })
                    .transition,
                ActivationTransition::Cancelled {
                    source: ChangeSource::Pointer,
                    reason: ActivationCancelReason::GestureClaimed(gesture),
                }
            );
            assert_eq!(
                machine.handle(ActivationInput::PointerUp {
                    pointer: POINTER,
                    button: PointerButton::PRIMARY,
                    inside: true,
                }),
                ActivationOutcome::ignored()
            );
        }
    }
}

//! Platform-neutral physical-chord and shortcut-scope matching.
//!
//! Platform adapters decide which physical chords implement user-facing conventions. Command
//! owners separately provide display bindings and execute the typed command returned here.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::num::NonZeroU32;

use crate::input::{ButtonState, KeyEvent, Modifiers, PhysicalKey};

/// Stable identity for one shortcut-scope generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShortcutScopeId {
    slot: NonZeroU32,
    generation: NonZeroU32,
}

impl ShortcutScopeId {
    pub const fn new(slot: NonZeroU32, generation: NonZeroU32) -> Self {
        Self { slot, generation }
    }

    pub const fn from_raw(slot: u32, generation: u32) -> Option<Self> {
        match (NonZeroU32::new(slot), NonZeroU32::new(generation)) {
            (Some(slot), Some(generation)) => Some(Self { slot, generation }),
            _ => None,
        }
    }

    pub const fn slot(self) -> u32 {
        self.slot.get()
    }

    pub const fn generation(self) -> u32 {
        self.generation.get()
    }
}

/// Whether an unmatched chord may continue to an enclosing shortcut scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShortcutScopePolicy {
    #[default]
    Bubble,
    Modal,
}

/// One active scope. Callers pass active scopes from innermost to outermost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveShortcutScope {
    pub id: ShortcutScopeId,
    pub policy: ShortcutScopePolicy,
}

impl ActiveShortcutScope {
    pub const fn bubble(id: ShortcutScopeId) -> Self {
        Self {
            id,
            policy: ShortcutScopePolicy::Bubble,
        }
    }

    pub const fn modal(id: ShortcutScopeId) -> Self {
        Self {
            id,
            policy: ShortcutScopePolicy::Modal,
        }
    }
}

/// Key transition on which a shortcut is eligible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShortcutTrigger {
    #[default]
    Pressed,
    Released,
}

impl ShortcutTrigger {
    const fn button_state(self) -> ButtonState {
        match self {
            Self::Pressed => ButtonState::Pressed,
            Self::Released => ButtonState::Released,
        }
    }
}

/// Exact physical key and modifier chord used only by the neutral matcher.
///
/// This is deliberately not a localized display binding and does not encode a platform's command
/// key convention. Gate 9 adapters map their logical/native keyboard data into this physical input
/// seam until the complete logical keyboard vocabulary exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShortcutChord {
    pub physical_key: PhysicalKey,
    pub modifiers: Modifiers,
    pub trigger: ShortcutTrigger,
}

impl ShortcutChord {
    pub const fn pressed(physical_key: PhysicalKey, modifiers: Modifiers) -> Self {
        Self {
            physical_key,
            modifiers,
            trigger: ShortcutTrigger::Pressed,
        }
    }

    pub const fn released(physical_key: PhysicalKey, modifiers: Modifiers) -> Self {
        Self {
            physical_key,
            modifiers,
            trigger: ShortcutTrigger::Released,
        }
    }

    pub fn matches(self, event: KeyEvent) -> bool {
        self.matches_borrowed(&event)
    }

    fn matches_borrowed(self, event: &KeyEvent) -> bool {
        self.physical_key.get() == event.physical_key.get()
            && self.modifiers.bits() == event.modifiers.bits()
            && matches!(
                (self.trigger.button_state(), event.state),
                (ButtonState::Pressed, ButtonState::Pressed)
                    | (ButtonState::Released, ButtonState::Released)
            )
    }
}

/// Whether repeated key-down events may resolve this binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShortcutRepeatPolicy {
    #[default]
    Suppress,
    Allow,
}

/// One controlled shortcut registration.
///
/// `K` identifies the registration generation and `C` is a typed command identifier. The matcher
/// returns `C`; it never invokes an action or stores an action factory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutBinding<K, C> {
    pub key: K,
    pub command: C,
    pub scope: ShortcutScopeId,
    pub chord: ShortcutChord,
    pub enabled: bool,
    pub priority: i16,
    pub repeat: ShortcutRepeatPolicy,
}

impl<K, C> ShortcutBinding<K, C> {
    pub const fn new(key: K, command: C, scope: ShortcutScopeId, chord: ShortcutChord) -> Self {
        Self {
            key,
            command,
            scope,
            chord,
            enabled: true,
            priority: 0,
            repeat: ShortcutRepeatPolicy::Suppress,
        }
    }
}

/// Result of matching one neutral keyboard event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutResolution<K, C> {
    NoMatch,
    Matched {
        binding: K,
        command: C,
        scope: ShortcutScopeId,
        chord: ShortcutChord,
    },
    Ambiguous {
        scope: ShortcutScopeId,
        chord: ShortcutChord,
        bindings: Vec<K>,
    },
    Blocked {
        scope: ShortcutScopeId,
    },
}

/// Rejected registration or active-scope input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutError<K> {
    DuplicateBinding(K),
    DuplicateActiveScope(ShortcutScopeId),
    ActiveScopeSlotCollision {
        first: ShortcutScopeId,
        second: ShortcutScopeId,
    },
}

/// Deterministic shortcut counters suitable for later per-view aggregation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutDiagnostics {
    pub binding_updates: u64,
    pub events: u64,
    pub matches: u64,
    pub ambiguities: u64,
    pub blocked: u64,
    pub unmatched: u64,
    pub disabled_skips: u64,
    pub repeat_skips: u64,
    pub failures: u64,
}

/// Pure scope-ordered matcher over caller-controlled bindings.
#[derive(Clone, Debug)]
pub struct ShortcutMatcher<K, C> {
    bindings: Vec<ShortcutBinding<K, C>>,
    diagnostics: ShortcutDiagnostics,
}

impl<K, C> Default for ShortcutMatcher<K, C> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            diagnostics: ShortcutDiagnostics::default(),
        }
    }
}

impl<K, C> ShortcutMatcher<K, C>
where
    K: Copy + Eq + Hash,
    C: Copy,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn diagnostics(&self) -> ShortcutDiagnostics {
        self.diagnostics
    }

    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Replaces the controlled binding snapshot atomically in canonical registration order.
    pub fn update_bindings(
        &mut self,
        bindings: impl IntoIterator<Item = ShortcutBinding<K, C>>,
    ) -> Result<(), ShortcutError<K>> {
        let bindings: Vec<_> = bindings.into_iter().collect();
        let mut keys = HashSet::with_capacity(bindings.len());
        for binding in &bindings {
            if !keys.insert(binding.key) {
                self.diagnostics.failures += 1;
                return Err(ShortcutError::DuplicateBinding(binding.key));
            }
        }
        self.bindings = bindings;
        self.diagnostics.binding_updates += 1;
        Ok(())
    }

    /// Resolves against an innermost-to-outermost active scope path.
    ///
    /// Scope proximity wins before numeric priority. Within the first scope containing eligible
    /// bindings, the highest priority wins; several bindings at that priority are reported as
    /// ambiguous. Disabled or repeat-suppressed bindings are ignored rather than executed.
    pub fn resolve(
        &mut self,
        event: KeyEvent,
        active_scopes: impl IntoIterator<Item = ActiveShortcutScope>,
    ) -> Result<ShortcutResolution<K, C>, ShortcutError<K>> {
        let active_scopes: Vec<_> = active_scopes.into_iter().collect();
        self.validate_scopes(&active_scopes)?;
        self.diagnostics.events += 1;

        for active_scope in active_scopes {
            let mut eligible = Vec::new();
            for binding in self.bindings.iter().filter(|binding| {
                binding.scope == active_scope.id && binding.chord.matches_borrowed(&event)
            }) {
                if !binding.enabled {
                    self.diagnostics.disabled_skips += 1;
                    continue;
                }
                if event.repeat && binding.repeat == ShortcutRepeatPolicy::Suppress {
                    self.diagnostics.repeat_skips += 1;
                    continue;
                }
                eligible.push(binding);
            }

            if let Some(priority) = eligible.iter().map(|binding| binding.priority).max() {
                let winners: Vec<_> = eligible
                    .into_iter()
                    .filter(|binding| binding.priority == priority)
                    .collect();
                if winners.len() == 1 {
                    let winner = winners[0];
                    self.diagnostics.matches += 1;
                    return Ok(ShortcutResolution::Matched {
                        binding: winner.key,
                        command: winner.command,
                        scope: winner.scope,
                        chord: winner.chord,
                    });
                }
                self.diagnostics.ambiguities += 1;
                return Ok(ShortcutResolution::Ambiguous {
                    scope: active_scope.id,
                    chord: winners[0].chord,
                    bindings: winners.iter().map(|binding| binding.key).collect(),
                });
            }

            if active_scope.policy == ShortcutScopePolicy::Modal {
                self.diagnostics.blocked += 1;
                return Ok(ShortcutResolution::Blocked {
                    scope: active_scope.id,
                });
            }
        }

        self.diagnostics.unmatched += 1;
        Ok(ShortcutResolution::NoMatch)
    }

    fn validate_scopes(
        &mut self,
        active_scopes: &[ActiveShortcutScope],
    ) -> Result<(), ShortcutError<K>> {
        let mut slots = HashMap::with_capacity(active_scopes.len());
        for scope in active_scopes {
            if let Some(first) = slots.insert(scope.id.slot(), scope.id) {
                self.diagnostics.failures += 1;
                return if first == scope.id {
                    Err(ShortcutError::DuplicateActiveScope(scope.id))
                } else {
                    Err(ShortcutError::ActiveScopeSlotCollision {
                        first,
                        second: scope.id,
                    })
                };
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAVE: PhysicalKey = PhysicalKey::new(10);
    const OTHER: PhysicalKey = PhysicalKey::new(11);

    fn scope(slot: u32, generation: u32) -> ShortcutScopeId {
        ShortcutScopeId::from_raw(slot, generation).unwrap()
    }

    fn event(key: PhysicalKey, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            physical_key: key,
            state: ButtonState::Pressed,
            repeat: false,
            modifiers,
            ..KeyEvent::new(key, ButtonState::Pressed)
        }
    }

    fn binding(key: u32, command: u32, scope: ShortcutScopeId) -> ShortcutBinding<u32, u32> {
        ShortcutBinding::new(
            key,
            command,
            scope,
            ShortcutChord::pressed(SAVE, Modifiers::CONTROL),
        )
    }

    #[test]
    fn chord_matches_exact_key_modifiers_and_transition() {
        let chord = ShortcutChord::pressed(SAVE, Modifiers::CONTROL.union(Modifiers::SHIFT));
        assert!(chord.matches(event(SAVE, Modifiers::CONTROL.union(Modifiers::SHIFT))));
        assert!(!chord.matches(event(SAVE, Modifiers::CONTROL)));
        assert!(
            !chord.matches(event(
                SAVE,
                Modifiers::CONTROL
                    .union(Modifiers::SHIFT)
                    .union(Modifiers::ALT)
            ))
        );
        assert!(!chord.matches(event(OTHER, Modifiers::CONTROL.union(Modifiers::SHIFT))));
        let mut released = event(SAVE, Modifiers::CONTROL.union(Modifiers::SHIFT));
        released.state = ButtonState::Released;
        assert!(!chord.matches(released.clone()));
        assert!(
            ShortcutChord::released(SAVE, Modifiers::CONTROL.union(Modifiers::SHIFT))
                .matches(released)
        );
    }

    #[test]
    fn physical_chord_matching_ignores_new_logical_text_location_and_synthetic_fields() {
        let chord = ShortcutChord::pressed(SAVE, Modifiers::CONTROL);
        let first = event(SAVE, Modifiers::CONTROL)
            .with_logical_key(crate::input::LogicalKey::Named(
                crate::input::NamedKey::Save,
            ))
            .with_location(crate::input::KeyLocation::Numpad)
            .with_synthetic(true);
        let second = event(SAVE, Modifiers::CONTROL)
            .with_logical_key(crate::input::LogicalKey::character("private").unwrap())
            .with_text(Some(crate::input::KeyText::new("private").unwrap()));

        assert!(chord.matches(first));
        assert!(chord.matches(second));
    }

    #[test]
    fn binding_updates_reject_duplicate_keys_atomically() {
        let root = scope(1, 1);
        let mut matcher = ShortcutMatcher::new();
        matcher.update_bindings([binding(1, 10, root)]).unwrap();
        assert_eq!(
            matcher.update_bindings([binding(2, 20, root), binding(2, 30, root)]),
            Err(ShortcutError::DuplicateBinding(2))
        );
        assert_eq!(matcher.binding_count(), 1);
        assert!(matches!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [ActiveShortcutScope::bubble(root)]
                )
                .unwrap(),
            ShortcutResolution::Matched { command: 10, .. }
        ));
    }

    #[test]
    fn innermost_eligible_scope_wins_before_outer_priority() {
        let inner = scope(2, 1);
        let outer = scope(1, 1);
        let mut inner_binding = binding(1, 100, inner);
        inner_binding.priority = -10;
        let mut outer_binding = binding(2, 200, outer);
        outer_binding.priority = 100;
        let mut matcher = ShortcutMatcher::new();
        matcher
            .update_bindings([outer_binding, inner_binding])
            .unwrap();
        assert!(matches!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [
                        ActiveShortcutScope::bubble(inner),
                        ActiveShortcutScope::bubble(outer),
                    ]
                )
                .unwrap(),
            ShortcutResolution::Matched {
                binding: 1,
                command: 100,
                scope: selected,
                ..
            } if selected == inner
        ));
    }

    #[test]
    fn disabled_inner_binding_is_ignored_and_outer_can_match() {
        let inner = scope(2, 1);
        let outer = scope(1, 1);
        let mut disabled = binding(1, 100, inner);
        disabled.enabled = false;
        let mut matcher = ShortcutMatcher::new();
        matcher
            .update_bindings([disabled, binding(2, 200, outer)])
            .unwrap();
        assert!(matches!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [
                        ActiveShortcutScope::bubble(inner),
                        ActiveShortcutScope::bubble(outer),
                    ]
                )
                .unwrap(),
            ShortcutResolution::Matched { command: 200, .. }
        ));
        assert_eq!(matcher.diagnostics().disabled_skips, 1);
    }

    #[test]
    fn highest_priority_wins_and_equal_priority_is_ambiguous() {
        let root = scope(1, 1);
        let mut low = binding(1, 10, root);
        low.priority = 1;
        let mut high = binding(2, 20, root);
        high.priority = 2;
        let mut matcher = ShortcutMatcher::new();
        matcher.update_bindings([low, high]).unwrap();
        assert!(matches!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [ActiveShortcutScope::bubble(root)]
                )
                .unwrap(),
            ShortcutResolution::Matched { binding: 2, .. }
        ));

        high.priority = 1;
        matcher.update_bindings([low, high]).unwrap();
        assert_eq!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [ActiveShortcutScope::bubble(root)]
                )
                .unwrap(),
            ShortcutResolution::Ambiguous {
                scope: root,
                chord: low.chord,
                bindings: vec![1, 2],
            }
        );
    }

    #[test]
    fn modal_scope_blocks_outer_match_when_it_has_no_eligible_binding() {
        let modal = scope(2, 1);
        let outer = scope(1, 1);
        let mut matcher = ShortcutMatcher::new();
        matcher.update_bindings([binding(1, 10, outer)]).unwrap();
        assert_eq!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [
                        ActiveShortcutScope::modal(modal),
                        ActiveShortcutScope::bubble(outer),
                    ]
                )
                .unwrap(),
            ShortcutResolution::Blocked { scope: modal }
        );
    }

    #[test]
    fn repeat_policy_is_per_binding_and_does_not_invent_platform_policy() {
        let root = scope(1, 1);
        let mut matcher = ShortcutMatcher::new();
        matcher.update_bindings([binding(1, 10, root)]).unwrap();
        let mut repeated = event(SAVE, Modifiers::CONTROL);
        repeated.repeat = true;
        assert_eq!(
            matcher
                .resolve(repeated.clone(), [ActiveShortcutScope::bubble(root)])
                .unwrap(),
            ShortcutResolution::NoMatch
        );
        let mut repeating = binding(1, 10, root);
        repeating.repeat = ShortcutRepeatPolicy::Allow;
        matcher.update_bindings([repeating]).unwrap();
        assert!(matches!(
            matcher
                .resolve(repeated, [ActiveShortcutScope::bubble(root)])
                .unwrap(),
            ShortcutResolution::Matched { command: 10, .. }
        ));
    }

    #[test]
    fn release_trigger_and_unmatched_events_are_distinct() {
        let root = scope(1, 1);
        let mut released_binding = binding(1, 10, root);
        released_binding.chord = ShortcutChord::released(SAVE, Modifiers::CONTROL);
        let mut matcher = ShortcutMatcher::new();
        matcher.update_bindings([released_binding]).unwrap();
        assert_eq!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [ActiveShortcutScope::bubble(root)]
                )
                .unwrap(),
            ShortcutResolution::NoMatch
        );
        let mut released = event(SAVE, Modifiers::CONTROL);
        released.state = ButtonState::Released;
        assert!(matches!(
            matcher
                .resolve(released, [ActiveShortcutScope::bubble(root)])
                .unwrap(),
            ShortcutResolution::Matched { command: 10, .. }
        ));
    }

    #[test]
    fn recycled_scope_generation_does_not_match_old_binding() {
        let old = scope(4, 1);
        let replacement = scope(4, 2);
        let mut matcher = ShortcutMatcher::new();
        matcher.update_bindings([binding(1, 10, old)]).unwrap();
        assert_eq!(
            matcher
                .resolve(
                    event(SAVE, Modifiers::CONTROL),
                    [ActiveShortcutScope::bubble(replacement)]
                )
                .unwrap(),
            ShortcutResolution::NoMatch
        );
    }

    #[test]
    fn active_scope_duplicates_and_generation_collisions_are_rejected() {
        let old = scope(4, 1);
        let replacement = scope(4, 2);
        let mut matcher: ShortcutMatcher<u32, u32> = ShortcutMatcher::new();
        assert_eq!(
            matcher.resolve(
                event(SAVE, Modifiers::CONTROL),
                [
                    ActiveShortcutScope::bubble(old),
                    ActiveShortcutScope::bubble(old),
                ]
            ),
            Err(ShortcutError::DuplicateActiveScope(old))
        );
        assert_eq!(
            matcher.resolve(
                event(SAVE, Modifiers::CONTROL),
                [
                    ActiveShortcutScope::bubble(old),
                    ActiveShortcutScope::bubble(replacement),
                ]
            ),
            Err(ShortcutError::ActiveScopeSlotCollision {
                first: old,
                second: replacement,
            })
        );
        assert_eq!(matcher.diagnostics().events, 0);
        assert_eq!(matcher.diagnostics().failures, 2);
    }
}

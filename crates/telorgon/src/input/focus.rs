use std::collections::HashSet;
use std::hash::Hash;
use std::num::NonZeroU32;

/// Stable identity for one focus scope generation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FocusScopeId {
    slot: NonZeroU32,
    generation: NonZeroU32,
}

impl FocusScopeId {
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

/// One target in canonical layout/reading order.
///
/// `K` remains owned by the mounted/runtime identity layer. It must include its generation so an
/// old target never aliases a replacement mounted in the same slot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FocusCandidate<K> {
    pub target: K,
    pub scope: FocusScopeId,
    pub focusable: bool,
    pub enabled: bool,
}

impl<K> FocusCandidate<K> {
    pub const fn new(target: K, scope: FocusScopeId) -> Self {
        Self {
            target,
            scope,
            focusable: true,
            enabled: true,
        }
    }

    pub const fn eligible(&self) -> bool {
        self.focusable && self.enabled
    }
}

/// Last neutral input class relevant to focus-indicator visibility.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FocusInputModality {
    Pointer,
    #[default]
    Keyboard,
    Directional,
    Accessibility,
}

impl FocusInputModality {
    const fn requires_indicator(self) -> bool {
        !matches!(self, Self::Pointer)
    }
}

/// View/user preference governing focus-indicator visibility.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FocusIndicatorPolicy {
    #[default]
    Automatic,
    Always,
}

/// Source of an explicit focus request.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FocusOrigin {
    Pointer,
    Keyboard,
    Directional,
    Accessibility,
    Programmatic,
}

/// One-dimensional outer focus traversal direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FocusTraversalDirection {
    Forward,
    Backward,
}

/// Behavior at the beginning or end of an active focus scope.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FocusTraversalEdge {
    #[default]
    Stop,
    Wrap,
}

/// Why primary focus moved.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FocusMoveReason {
    Requested(FocusOrigin),
    Traversal(FocusTraversalDirection),
    ScopeEntered,
    ScopeRestored,
    CandidateRemoved,
}

/// Why primary focus was cleared.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FocusClearReason {
    Explicit,
    ViewDeactivated,
    ScopeEnteredEmpty,
    ScopeRestoreUnavailable,
    CandidateRemoved,
}

/// Observable result of one accepted focus operation.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum FocusChange<K> {
    #[default]
    Unchanged,
    Moved {
        previous: Option<K>,
        current: K,
        visible: bool,
        reason: FocusMoveReason,
    },
    Cleared {
        previous: K,
        reason: FocusClearReason,
    },
    VisibilityChanged {
        target: K,
        visible: bool,
    },
    Boundary {
        direction: FocusTraversalDirection,
    },
}

/// Rejected focus update. Rejections never mutate candidates, scopes, or primary focus.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FocusError<K> {
    DuplicateCandidate(K),
    UnknownTarget(K),
    IneligibleTarget(K),
    TargetOutsideActiveScope {
        target: K,
        active_scope: FocusScopeId,
    },
    ScopeAlreadyActive(FocusScopeId),
    ScopeSlotStillActive {
        requested: FocusScopeId,
        active: FocusScopeId,
    },
    ScopeNotActive {
        requested: FocusScopeId,
        active: FocusScopeId,
    },
    CannotLeaveRootScope,
}

/// Deterministic counters suitable for aggregation by the owning view/runtime.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusDiagnostics {
    pub moves: u64,
    pub restores: u64,
    pub clears: u64,
    pub visibility_changes: u64,
    pub boundaries: u64,
    pub failures: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct FocusRestore<K> {
    target: K,
    visible: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct FocusScopeFrame<K> {
    id: FocusScopeId,
    edge: FocusTraversalEdge,
    restore: Option<FocusRestore<K>>,
}

/// Source-neutral primary-focus, scope, traversal, restoration, and focus-visible owner.
///
/// Candidate order is supplied by canonical layout/reading order. Directional composite navigation,
/// native focus, semantic focus, reveal/scroll requests, and mounted-node mutation remain separate
/// responsibilities.
#[derive(Clone, Debug)]
pub struct FocusStateMachine<K> {
    candidates: Vec<FocusCandidate<K>>,
    scopes: Vec<FocusScopeFrame<K>>,
    focused: Option<K>,
    focus_visible: bool,
    modality: FocusInputModality,
    indicator_policy: FocusIndicatorPolicy,
    diagnostics: FocusDiagnostics,
}

impl<K> FocusStateMachine<K>
where
    K: Copy + Eq + Hash,
{
    pub fn new(
        root_scope: FocusScopeId,
        root_edge: FocusTraversalEdge,
        indicator_policy: FocusIndicatorPolicy,
    ) -> Self {
        Self {
            candidates: Vec::new(),
            scopes: vec![FocusScopeFrame {
                id: root_scope,
                edge: root_edge,
                restore: None,
            }],
            focused: None,
            focus_visible: false,
            modality: FocusInputModality::default(),
            indicator_policy,
            diagnostics: FocusDiagnostics::default(),
        }
    }

    pub fn active_scope(&self) -> FocusScopeId {
        self.scopes
            .last()
            .expect("focus state always retains its root scope")
            .id
    }

    pub const fn focused(&self) -> Option<K> {
        self.focused
    }

    pub const fn focus_visible(&self) -> bool {
        self.focused.is_some() && self.focus_visible
    }

    pub const fn modality(&self) -> FocusInputModality {
        self.modality
    }

    pub const fn indicator_policy(&self) -> FocusIndicatorPolicy {
        self.indicator_policy
    }

    pub const fn diagnostics(&self) -> FocusDiagnostics {
        self.diagnostics
    }

    pub fn candidates(&self) -> &[FocusCandidate<K>] {
        &self.candidates
    }

    /// Atomically replaces canonical candidate order and reconciles an invalid primary target.
    ///
    /// A removed target moves to the nearest surviving old successor, then predecessor. A newly
    /// mounted replacement generation is never selected merely because it occupies the same slot.
    pub fn update_candidates(
        &mut self,
        candidates: Vec<FocusCandidate<K>>,
    ) -> Result<FocusChange<K>, FocusError<K>> {
        let mut seen = HashSet::with_capacity(candidates.len());
        for candidate in &candidates {
            if !seen.insert(candidate.target) {
                self.diagnostics.failures += 1;
                return Err(FocusError::DuplicateCandidate(candidate.target));
            }
        }

        let previous_candidates = std::mem::replace(&mut self.candidates, candidates);
        let Some(focused) = self.focused else {
            return Ok(FocusChange::Unchanged);
        };
        if self.is_eligible_in_scope(focused, self.active_scope()) {
            return Ok(FocusChange::Unchanged);
        }

        let replacement = previous_candidates
            .iter()
            .position(|candidate| candidate.target == focused)
            .and_then(|index| {
                previous_candidates[index + 1..]
                    .iter()
                    .chain(previous_candidates[..index].iter().rev())
                    .find_map(|candidate| {
                        self.is_eligible_in_scope(candidate.target, self.active_scope())
                            .then_some(candidate.target)
                    })
            });

        if let Some(target) = replacement {
            Ok(self.move_to(
                target,
                FocusMoveReason::CandidateRemoved,
                self.focus_visible,
            ))
        } else {
            Ok(self.clear(FocusClearReason::CandidateRemoved))
        }
    }

    /// Notes an input modality and updates the current focus indicator without moving focus.
    pub fn note_input(&mut self, modality: FocusInputModality) -> FocusChange<K> {
        self.modality = modality;
        self.update_visibility(self.desired_visibility())
    }

    pub fn set_indicator_policy(&mut self, policy: FocusIndicatorPolicy) -> FocusChange<K> {
        self.indicator_policy = policy;
        self.update_visibility(self.desired_visibility())
    }

    pub fn request_focus(
        &mut self,
        target: K,
        origin: FocusOrigin,
    ) -> Result<FocusChange<K>, FocusError<K>> {
        let Some(candidate) = self
            .candidates
            .iter()
            .find(|candidate| candidate.target == target)
            .copied()
        else {
            self.diagnostics.failures += 1;
            return Err(FocusError::UnknownTarget(target));
        };
        if candidate.scope != self.active_scope() {
            self.diagnostics.failures += 1;
            return Err(FocusError::TargetOutsideActiveScope {
                target,
                active_scope: self.active_scope(),
            });
        }
        if !candidate.eligible() {
            self.diagnostics.failures += 1;
            return Err(FocusError::IneligibleTarget(target));
        }

        self.apply_origin(origin);
        let visible = self.desired_visibility();
        if self.focused == Some(target) {
            return Ok(self.update_visibility(visible));
        }
        Ok(self.move_to(target, FocusMoveReason::Requested(origin), visible))
    }

    pub fn traverse(&mut self, direction: FocusTraversalDirection) -> FocusChange<K> {
        self.modality = FocusInputModality::Keyboard;
        let active_scope = self.active_scope();
        let mut eligible = self
            .candidates
            .iter()
            .filter(|candidate| candidate.scope == active_scope && candidate.eligible());
        let first = eligible.next().map(|candidate| candidate.target);
        let last = eligible.last().map(|candidate| candidate.target).or(first);
        let Some(edge_target) = (match direction {
            FocusTraversalDirection::Forward => first,
            FocusTraversalDirection::Backward => last,
        }) else {
            self.diagnostics.boundaries += 1;
            return FocusChange::Boundary { direction };
        };

        let next = self.focused.and_then(|focused| {
            let index = self
                .candidates
                .iter()
                .position(|candidate| candidate.target == focused)?;
            match direction {
                FocusTraversalDirection::Forward => self.candidates[index + 1..]
                    .iter()
                    .find(|candidate| candidate.scope == active_scope && candidate.eligible())
                    .map(|candidate| candidate.target),
                FocusTraversalDirection::Backward => self.candidates[..index]
                    .iter()
                    .rev()
                    .find(|candidate| candidate.scope == active_scope && candidate.eligible())
                    .map(|candidate| candidate.target),
            }
        });

        let target = match (self.focused, next) {
            (None, _) => edge_target,
            (_, Some(target)) => target,
            (Some(current), None)
                if self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.edge == FocusTraversalEdge::Wrap)
                    && edge_target != current =>
            {
                edge_target
            }
            (Some(_), None) => {
                self.diagnostics.boundaries += 1;
                return FocusChange::Boundary { direction };
            }
        };
        self.move_to(
            target,
            FocusMoveReason::Traversal(direction),
            self.desired_visibility(),
        )
    }

    pub fn enter_scope(
        &mut self,
        scope: FocusScopeId,
        edge: FocusTraversalEdge,
        preferred: Option<K>,
        origin: FocusOrigin,
    ) -> Result<FocusChange<K>, FocusError<K>> {
        if let Some(active) = self.scopes.iter().find(|active| active.id == scope) {
            self.diagnostics.failures += 1;
            return Err(FocusError::ScopeAlreadyActive(active.id));
        }
        if let Some(active) = self
            .scopes
            .iter()
            .find(|active| active.id.slot() == scope.slot())
        {
            self.diagnostics.failures += 1;
            return Err(FocusError::ScopeSlotStillActive {
                requested: scope,
                active: active.id,
            });
        }

        let target = if let Some(preferred) = preferred {
            let Some(candidate) = self
                .candidates
                .iter()
                .find(|candidate| candidate.target == preferred)
            else {
                self.diagnostics.failures += 1;
                return Err(FocusError::UnknownTarget(preferred));
            };
            if candidate.scope != scope || !candidate.eligible() {
                self.diagnostics.failures += 1;
                return Err(FocusError::IneligibleTarget(preferred));
            }
            Some(preferred)
        } else {
            self.candidates
                .iter()
                .find(|candidate| candidate.scope == scope && candidate.eligible())
                .map(|candidate| candidate.target)
        };

        let previous = self.focused;
        let restore = previous.map(|target| FocusRestore {
            target,
            visible: self.focus_visible,
        });
        self.scopes.push(FocusScopeFrame {
            id: scope,
            edge,
            restore,
        });
        self.apply_origin(origin);
        if let Some(target) = target {
            Ok(self.move_to(
                target,
                FocusMoveReason::ScopeEntered,
                self.desired_visibility(),
            ))
        } else if previous.is_some() {
            Ok(self.clear(FocusClearReason::ScopeEnteredEmpty))
        } else {
            Ok(FocusChange::Unchanged)
        }
    }

    pub fn leave_scope(&mut self, scope: FocusScopeId) -> Result<FocusChange<K>, FocusError<K>> {
        if self.scopes.len() == 1 && self.active_scope() == scope {
            self.diagnostics.failures += 1;
            return Err(FocusError::CannotLeaveRootScope);
        }
        if self.active_scope() != scope {
            self.diagnostics.failures += 1;
            return Err(FocusError::ScopeNotActive {
                requested: scope,
                active: self.active_scope(),
            });
        }

        let frame = self
            .scopes
            .pop()
            .expect("validated non-root focus scope exists");
        if let Some(restore) = frame.restore
            && self.is_eligible_in_scope(restore.target, self.active_scope())
        {
            self.diagnostics.restores += 1;
            let visible = if self.indicator_policy == FocusIndicatorPolicy::Always {
                true
            } else {
                restore.visible
            };
            return Ok(self.move_to(restore.target, FocusMoveReason::ScopeRestored, visible));
        }
        if self.focused.is_some() {
            Ok(self.clear(FocusClearReason::ScopeRestoreUnavailable))
        } else {
            Ok(FocusChange::Unchanged)
        }
    }

    pub fn clear_focus(&mut self, reason: FocusClearReason) -> FocusChange<K> {
        self.clear(reason)
    }

    fn apply_origin(&mut self, origin: FocusOrigin) {
        self.modality = match origin {
            FocusOrigin::Pointer => FocusInputModality::Pointer,
            FocusOrigin::Keyboard => FocusInputModality::Keyboard,
            FocusOrigin::Directional => FocusInputModality::Directional,
            FocusOrigin::Accessibility => FocusInputModality::Accessibility,
            FocusOrigin::Programmatic => return,
        };
    }

    fn desired_visibility(&self) -> bool {
        self.indicator_policy == FocusIndicatorPolicy::Always || self.modality.requires_indicator()
    }

    fn is_eligible_in_scope(&self, target: K, scope: FocusScopeId) -> bool {
        self.candidates.iter().any(|candidate| {
            candidate.target == target && candidate.scope == scope && candidate.eligible()
        })
    }

    fn move_to(&mut self, target: K, reason: FocusMoveReason, visible: bool) -> FocusChange<K> {
        let previous = self.focused.replace(target);
        self.focus_visible = visible;
        if previous == Some(target) {
            return FocusChange::Unchanged;
        }
        self.diagnostics.moves += 1;
        FocusChange::Moved {
            previous,
            current: target,
            visible,
            reason,
        }
    }

    fn clear(&mut self, reason: FocusClearReason) -> FocusChange<K> {
        let Some(previous) = self.focused.take() else {
            return FocusChange::Unchanged;
        };
        self.focus_visible = false;
        self.diagnostics.clears += 1;
        FocusChange::Cleared { previous, reason }
    }

    fn update_visibility(&mut self, visible: bool) -> FocusChange<K> {
        let Some(target) = self.focused else {
            return FocusChange::Unchanged;
        };
        if self.focus_visible == visible {
            return FocusChange::Unchanged;
        }
        self.focus_visible = visible;
        self.diagnostics.visibility_changes += 1;
        FocusChange::VisibilityChanged { target, visible }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    struct Target {
        slot: u32,
        generation: u32,
    }

    const fn target(slot: u32, generation: u32) -> Target {
        Target { slot, generation }
    }

    fn scope(slot: u32, generation: u32) -> FocusScopeId {
        FocusScopeId::from_raw(slot, generation).unwrap()
    }

    fn candidate(target: Target, scope: FocusScopeId) -> FocusCandidate<Target> {
        FocusCandidate::new(target, scope)
    }

    fn root_machine() -> FocusStateMachine<Target> {
        FocusStateMachine::new(
            scope(1, 1),
            FocusTraversalEdge::Stop,
            FocusIndicatorPolicy::Automatic,
        )
    }

    #[test]
    fn scope_ids_are_nonzero_and_generation_aware() {
        assert_eq!(FocusScopeId::from_raw(0, 1), None);
        assert_eq!(FocusScopeId::from_raw(1, 0), None);
        assert_ne!(scope(1, 1), scope(1, 2));
        assert_eq!(scope(4, 7).slot(), 4);
        assert_eq!(scope(4, 7).generation(), 7);
    }

    #[test]
    fn candidate_updates_reject_duplicates_atomically() {
        let root = scope(1, 1);
        let mut machine = root_machine();
        let initial = vec![candidate(target(1, 1), root)];
        machine.update_candidates(initial.clone()).unwrap();
        assert_eq!(
            machine.update_candidates(vec![
                candidate(target(2, 1), root),
                candidate(target(2, 1), root),
            ]),
            Err(FocusError::DuplicateCandidate(target(2, 1)))
        );
        assert_eq!(machine.candidates(), initial);
        assert_eq!(machine.diagnostics().failures, 1);
    }

    #[test]
    fn traversal_uses_canonical_order_skips_ineligible_and_stops_at_edges() {
        let root = scope(1, 1);
        let mut machine = root_machine();
        let mut disabled = candidate(target(2, 1), root);
        disabled.enabled = false;
        machine
            .update_candidates(vec![
                candidate(target(1, 1), root),
                disabled,
                candidate(target(3, 1), root),
            ])
            .unwrap();
        assert!(matches!(
            machine.traverse(FocusTraversalDirection::Forward),
            FocusChange::Moved {
                current: Target { slot: 1, .. },
                visible: true,
                ..
            }
        ));
        assert!(matches!(
            machine.traverse(FocusTraversalDirection::Forward),
            FocusChange::Moved {
                current: Target { slot: 3, .. },
                ..
            }
        ));
        assert_eq!(
            machine.traverse(FocusTraversalDirection::Forward),
            FocusChange::Boundary {
                direction: FocusTraversalDirection::Forward,
            }
        );
        assert!(matches!(
            machine.traverse(FocusTraversalDirection::Backward),
            FocusChange::Moved {
                current: Target { slot: 1, .. },
                ..
            }
        ));
    }

    #[test]
    fn wrapping_is_explicit_per_scope() {
        let root = scope(1, 1);
        let mut machine = FocusStateMachine::new(
            root,
            FocusTraversalEdge::Wrap,
            FocusIndicatorPolicy::Automatic,
        );
        machine
            .update_candidates(vec![
                candidate(target(1, 1), root),
                candidate(target(2, 1), root),
            ])
            .unwrap();
        machine.traverse(FocusTraversalDirection::Backward);
        assert_eq!(machine.focused(), Some(target(2, 1)));
        machine.traverse(FocusTraversalDirection::Forward);
        assert_eq!(machine.focused(), Some(target(1, 1)));
    }

    #[test]
    fn focus_visible_tracks_modality_and_always_policy() {
        let root = scope(1, 1);
        let mut machine = root_machine();
        machine
            .update_candidates(vec![candidate(target(1, 1), root)])
            .unwrap();
        machine
            .request_focus(target(1, 1), FocusOrigin::Pointer)
            .unwrap();
        assert!(!machine.focus_visible());
        assert!(matches!(
            machine.note_input(FocusInputModality::Keyboard),
            FocusChange::VisibilityChanged { visible: true, .. }
        ));
        assert!(matches!(
            machine.note_input(FocusInputModality::Pointer),
            FocusChange::VisibilityChanged { visible: false, .. }
        ));
        machine.set_indicator_policy(FocusIndicatorPolicy::Always);
        assert!(machine.focus_visible());
        assert_eq!(
            machine.note_input(FocusInputModality::Pointer),
            FocusChange::Unchanged
        );
    }

    #[test]
    fn invalid_requests_do_not_move_focus() {
        let root = scope(1, 1);
        let nested = scope(2, 1);
        let mut disabled = candidate(target(2, 1), root);
        disabled.enabled = false;
        let mut machine = root_machine();
        machine
            .update_candidates(vec![
                candidate(target(1, 1), root),
                disabled,
                candidate(target(3, 1), nested),
            ])
            .unwrap();
        machine
            .request_focus(target(1, 1), FocusOrigin::Keyboard)
            .unwrap();
        assert_eq!(
            machine.request_focus(target(9, 1), FocusOrigin::Keyboard),
            Err(FocusError::UnknownTarget(target(9, 1)))
        );
        assert_eq!(
            machine.request_focus(target(2, 1), FocusOrigin::Keyboard),
            Err(FocusError::IneligibleTarget(target(2, 1)))
        );
        assert!(matches!(
            machine.request_focus(target(3, 1), FocusOrigin::Keyboard),
            Err(FocusError::TargetOutsideActiveScope { .. })
        ));
        assert_eq!(machine.focused(), Some(target(1, 1)));
    }

    #[test]
    fn nested_scope_restores_exact_parent_target_and_visibility() {
        let root = scope(1, 1);
        let nested = scope(2, 1);
        let mut machine = root_machine();
        machine
            .update_candidates(vec![
                candidate(target(1, 1), root),
                candidate(target(2, 1), nested),
            ])
            .unwrap();
        machine
            .request_focus(target(1, 1), FocusOrigin::Keyboard)
            .unwrap();
        machine
            .enter_scope(
                nested,
                FocusTraversalEdge::Wrap,
                Some(target(2, 1)),
                FocusOrigin::Pointer,
            )
            .unwrap();
        assert_eq!(machine.focused(), Some(target(2, 1)));
        assert!(!machine.focus_visible());
        assert!(matches!(
            machine.leave_scope(nested).unwrap(),
            FocusChange::Moved {
                current: Target { slot: 1, .. },
                visible: true,
                reason: FocusMoveReason::ScopeRestored,
                ..
            }
        ));
        assert_eq!(machine.diagnostics().restores, 1);
    }

    #[test]
    fn unavailable_scope_restore_clears_instead_of_guessing_a_replacement() {
        let root = scope(1, 1);
        let nested = scope(2, 1);
        let old = target(1, 1);
        let replacement_generation = target(1, 2);
        let mut machine = root_machine();
        machine
            .update_candidates(vec![candidate(old, root), candidate(target(2, 1), nested)])
            .unwrap();
        machine.request_focus(old, FocusOrigin::Keyboard).unwrap();
        machine
            .enter_scope(
                nested,
                FocusTraversalEdge::Wrap,
                None,
                FocusOrigin::Programmatic,
            )
            .unwrap();
        machine
            .update_candidates(vec![
                candidate(replacement_generation, root),
                candidate(target(2, 1), nested),
            ])
            .unwrap();
        assert!(matches!(
            machine.leave_scope(nested).unwrap(),
            FocusChange::Cleared {
                reason: FocusClearReason::ScopeRestoreUnavailable,
                ..
            }
        ));
        assert_eq!(machine.focused(), None);
    }

    #[test]
    fn focused_removal_prefers_old_successor_then_predecessor() {
        let root = scope(1, 1);
        let one = target(1, 1);
        let two = target(2, 1);
        let three = target(3, 1);
        let mut machine = root_machine();
        machine
            .update_candidates(vec![
                candidate(one, root),
                candidate(two, root),
                candidate(three, root),
            ])
            .unwrap();
        machine.request_focus(two, FocusOrigin::Keyboard).unwrap();
        machine
            .update_candidates(vec![candidate(one, root), candidate(three, root)])
            .unwrap();
        assert_eq!(machine.focused(), Some(three));
        machine
            .update_candidates(vec![candidate(one, root)])
            .unwrap();
        assert_eq!(machine.focused(), Some(one));
    }

    #[test]
    fn replacement_generation_is_never_implicitly_focused() {
        let root = scope(1, 1);
        let mut machine = root_machine();
        machine
            .update_candidates(vec![candidate(target(4, 1), root)])
            .unwrap();
        machine
            .request_focus(target(4, 1), FocusOrigin::Keyboard)
            .unwrap();
        assert!(matches!(
            machine
                .update_candidates(vec![candidate(target(4, 2), root)])
                .unwrap(),
            FocusChange::Cleared {
                reason: FocusClearReason::CandidateRemoved,
                ..
            }
        ));
        assert_eq!(machine.focused(), None);
    }

    #[test]
    fn stale_scope_generation_cannot_exit_or_nest_over_a_live_slot() {
        let root = scope(1, 1);
        let nested = scope(2, 1);
        let reused = scope(2, 2);
        let mut machine = root_machine();
        machine
            .update_candidates(vec![candidate(target(2, 1), nested)])
            .unwrap();
        machine
            .enter_scope(
                nested,
                FocusTraversalEdge::Wrap,
                None,
                FocusOrigin::Keyboard,
            )
            .unwrap();
        assert!(matches!(
            machine.leave_scope(reused),
            Err(FocusError::ScopeNotActive { .. })
        ));
        assert!(matches!(
            machine.enter_scope(
                reused,
                FocusTraversalEdge::Wrap,
                None,
                FocusOrigin::Keyboard,
            ),
            Err(FocusError::ScopeSlotStillActive { .. })
        ));
        assert_eq!(machine.active_scope(), nested);
        assert_eq!(
            machine.leave_scope(root),
            Err(FocusError::ScopeNotActive {
                requested: root,
                active: nested,
            })
        );
    }

    #[test]
    fn root_scope_cannot_be_left() {
        let root = scope(1, 1);
        let mut machine = root_machine();
        assert_eq!(
            machine.leave_scope(root),
            Err(FocusError::CannotLeaveRootScope)
        );
    }
}

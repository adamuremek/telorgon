//! Source-neutral active-descendant and one-dimensional composite navigation.
//!
//! The machine owns transient highlight and re-entry history only. Selection remains a controlled
//! value supplied by the caller; operations that should select an item return a typed request
//! instead of committing application state.

use std::collections::HashSet;
use std::hash::Hash;

use crate::input::ChangeSource;

/// One keyed item in canonical reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositeItem<K> {
    pub key: K,
    pub enabled: bool,
}

/// Whether a component pattern exposes disabled items during directional navigation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisabledItemPolicy {
    /// Disabled items are skipped entirely.
    #[default]
    Skip,
    /// Disabled items may be highlighted for discovery, but can never request selection.
    Include,
}

/// The directional axes accepted by a one-dimensional composite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompositeOrientation {
    #[default]
    Horizontal,
    Vertical,
    /// Both arrow-key pairs address the same canonical order.
    Both,
}

/// Behavior when directional navigation reaches the first or last eligible item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompositeEdgeBehavior {
    #[default]
    Stop,
    Wrap,
}

/// Whether moving the active descendant also requests a controlled selection change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompositeSelectionBehavior {
    #[default]
    Independent,
    FollowsHighlight,
}

/// Logical text and reading direction supplied by the current environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WritingDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

/// Fixed component-pattern policy for a composite state machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompositeNavigationPolicy {
    pub orientation: CompositeOrientation,
    pub edge_behavior: CompositeEdgeBehavior,
    pub disabled_items: DisabledItemPolicy,
    pub selection: CompositeSelectionBehavior,
}

/// A source-neutral navigation command after native key conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeNavigationCommand {
    Previous,
    Next,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// Why an item became the active descendant on composite entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeEntryReason {
    Restored,
    Selected,
    FirstEnabled,
    NoNavigableItem,
}

/// Why an active descendant changed after entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeHighlightReason {
    Navigation(CompositeNavigationCommand),
    Programmatic,
    ItemsChanged,
}

/// The focus target contributed by the composite to its owning focus mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeFocusTarget<K> {
    Root,
    Item(K),
}

/// A requested controlled selection change. Returning this value does not commit selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositeSelectionRequest<K> {
    pub key: K,
    pub source: ChangeSource,
}

/// Observable result of a composite transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeChange<K> {
    Unchanged,
    Entered {
        target: CompositeFocusTarget<K>,
        reason: CompositeEntryReason,
    },
    Left {
        previous: CompositeFocusTarget<K>,
    },
    Highlighted {
        previous: CompositeFocusTarget<K>,
        current: K,
        reason: CompositeHighlightReason,
        selection_request: Option<CompositeSelectionRequest<K>>,
    },
    Rooted {
        previous: K,
        reason: CompositeHighlightReason,
    },
    Boundary {
        current: CompositeFocusTarget<K>,
        command: CompositeNavigationCommand,
    },
    Ignored {
        command: CompositeNavigationCommand,
    },
}

/// Rejected transitions. Rejections leave the complete machine state unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeError<K> {
    DuplicateKey(K),
    AlreadyEntered,
    NotEntered,
    UnknownItem(K),
    ItemNotNavigable(K),
    NoActiveDescendant,
    ActiveDescendantDisabled(K),
}

/// Deterministic counters suitable for later aggregation into per-view diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompositeDiagnostics {
    pub entries: u64,
    pub restores: u64,
    pub highlight_changes: u64,
    pub selection_requests: u64,
    pub root_fallbacks: u64,
    pub boundaries: u64,
    pub failures: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Backward,
    Forward,
    First,
    Last,
    Ignored,
}

/// Pure active-descendant transition owner for a one-dimensional composite.
#[derive(Clone, Debug)]
pub struct CompositeStateMachine<K> {
    policy: CompositeNavigationPolicy,
    items: Vec<CompositeItem<K>>,
    entered: bool,
    active_descendant: Option<K>,
    last_active_descendant: Option<K>,
    diagnostics: CompositeDiagnostics,
}

impl<K> CompositeStateMachine<K>
where
    K: Copy + Eq + Hash,
{
    pub fn new(policy: CompositeNavigationPolicy) -> Self {
        Self {
            policy,
            items: Vec::new(),
            entered: false,
            active_descendant: None,
            last_active_descendant: None,
            diagnostics: CompositeDiagnostics::default(),
        }
    }

    pub fn policy(&self) -> CompositeNavigationPolicy {
        self.policy
    }

    pub fn is_entered(&self) -> bool {
        self.entered
    }

    pub fn active_descendant(&self) -> Option<K> {
        self.active_descendant
    }

    pub fn last_active_descendant(&self) -> Option<K> {
        self.last_active_descendant
    }

    pub fn active_target(&self) -> Option<CompositeFocusTarget<K>> {
        self.entered.then(|| {
            self.active_descendant
                .map_or(CompositeFocusTarget::Root, CompositeFocusTarget::Item)
        })
    }

    pub fn diagnostics(&self) -> CompositeDiagnostics {
        self.diagnostics
    }

    /// Replaces canonical item order atomically.
    ///
    /// If the active item disappears or becomes unnavigable, recovery considers only surviving
    /// keys from the old order: the first old successor, then the first old predecessor. A newly
    /// inserted key (including a recycled slot with a new generation) is never chosen implicitly.
    pub fn update_items(
        &mut self,
        items: impl IntoIterator<Item = CompositeItem<K>>,
    ) -> Result<CompositeChange<K>, CompositeError<K>> {
        let items: Vec<_> = items.into_iter().collect();
        let mut keys = HashSet::with_capacity(items.len());
        for item in &items {
            if !keys.insert(item.key) {
                self.diagnostics.failures += 1;
                return Err(CompositeError::DuplicateKey(item.key));
            }
        }

        let previous_items = std::mem::replace(&mut self.items, items);
        let Some(previous) = self.active_descendant else {
            if self
                .last_active_descendant
                .is_some_and(|key| !self.is_navigable(key))
            {
                self.last_active_descendant = None;
            }
            return Ok(CompositeChange::Unchanged);
        };

        if self.is_navigable(previous) {
            return Ok(CompositeChange::Unchanged);
        }

        let replacement = previous_items
            .iter()
            .position(|item| item.key == previous)
            .and_then(|index| {
                previous_items[index + 1..]
                    .iter()
                    .find(|item| self.is_navigable(item.key))
                    .or_else(|| {
                        previous_items[..index]
                            .iter()
                            .rev()
                            .find(|item| self.is_navigable(item.key))
                    })
                    .map(|item| item.key)
            });

        self.active_descendant = replacement;
        self.last_active_descendant = replacement;
        self.diagnostics.highlight_changes += 1;
        match replacement {
            Some(current) => Ok(CompositeChange::Highlighted {
                previous: CompositeFocusTarget::Item(previous),
                current,
                reason: CompositeHighlightReason::ItemsChanged,
                selection_request: None,
            }),
            None => {
                self.diagnostics.root_fallbacks += 1;
                Ok(CompositeChange::Rooted {
                    previous,
                    reason: CompositeHighlightReason::ItemsChanged,
                })
            }
        }
    }

    /// Enters the composite, using controlled selection only as a fallback hint.
    pub fn enter(&mut self, selected: Option<K>) -> Result<CompositeChange<K>, CompositeError<K>> {
        if self.entered {
            self.diagnostics.failures += 1;
            return Err(CompositeError::AlreadyEntered);
        }

        let (active_descendant, reason) = if let Some(key) = self
            .last_active_descendant
            .filter(|key| self.is_navigable(*key))
        {
            self.diagnostics.restores += 1;
            (Some(key), CompositeEntryReason::Restored)
        } else if let Some(key) = selected.filter(|key| self.is_navigable(*key)) {
            (Some(key), CompositeEntryReason::Selected)
        } else if let Some(key) = self
            .items
            .iter()
            .find_map(|item| item.enabled.then_some(item.key))
        {
            (Some(key), CompositeEntryReason::FirstEnabled)
        } else {
            (None, CompositeEntryReason::NoNavigableItem)
        };

        self.entered = true;
        self.active_descendant = active_descendant;
        self.last_active_descendant = active_descendant;
        self.diagnostics.entries += 1;
        Ok(CompositeChange::Entered {
            target: active_descendant
                .map_or(CompositeFocusTarget::Root, CompositeFocusTarget::Item),
            reason,
        })
    }

    pub fn leave(&mut self) -> Result<CompositeChange<K>, CompositeError<K>> {
        if !self.entered {
            self.diagnostics.failures += 1;
            return Err(CompositeError::NotEntered);
        }

        let previous = self
            .active_descendant
            .map_or(CompositeFocusTarget::Root, CompositeFocusTarget::Item);
        self.last_active_descendant = self.active_descendant;
        self.active_descendant = None;
        self.entered = false;
        Ok(CompositeChange::Left { previous })
    }

    /// Sets an active descendant without changing controlled selection.
    pub fn set_active_descendant(
        &mut self,
        key: K,
    ) -> Result<CompositeChange<K>, CompositeError<K>> {
        if !self.entered {
            self.diagnostics.failures += 1;
            return Err(CompositeError::NotEntered);
        }
        if !self.contains(key) {
            self.diagnostics.failures += 1;
            return Err(CompositeError::UnknownItem(key));
        }
        if !self.is_navigable(key) {
            self.diagnostics.failures += 1;
            return Err(CompositeError::ItemNotNavigable(key));
        }
        if self.active_descendant == Some(key) {
            return Ok(CompositeChange::Unchanged);
        }

        let previous = self
            .active_descendant
            .map_or(CompositeFocusTarget::Root, CompositeFocusTarget::Item);
        self.active_descendant = Some(key);
        self.last_active_descendant = Some(key);
        self.diagnostics.highlight_changes += 1;
        Ok(CompositeChange::Highlighted {
            previous,
            current: key,
            reason: CompositeHighlightReason::Programmatic,
            selection_request: None,
        })
    }

    /// Applies directional, Home, or End navigation in canonical item order.
    pub fn navigate(
        &mut self,
        command: CompositeNavigationCommand,
        direction: WritingDirection,
    ) -> Result<CompositeChange<K>, CompositeError<K>> {
        if !self.entered {
            self.diagnostics.failures += 1;
            return Err(CompositeError::NotEntered);
        }

        let step = self.resolve_step(command, direction);
        if step == Step::Ignored {
            return Ok(CompositeChange::Ignored { command });
        }

        let eligible: Vec<K> = self
            .items
            .iter()
            .filter_map(|item| self.is_navigable(item.key).then_some(item.key))
            .collect();
        let current_index = self
            .active_descendant
            .and_then(|current| eligible.iter().position(|key| *key == current));

        let next = match step {
            Step::First => eligible.first().copied(),
            Step::Last => eligible.last().copied(),
            Step::Forward => match current_index {
                Some(index) if index + 1 < eligible.len() => Some(eligible[index + 1]),
                Some(_) if self.policy.edge_behavior == CompositeEdgeBehavior::Wrap => {
                    eligible.first().copied()
                }
                None => eligible.first().copied(),
                Some(_) => None,
            },
            Step::Backward => match current_index {
                Some(index) if index > 0 => Some(eligible[index - 1]),
                Some(_) if self.policy.edge_behavior == CompositeEdgeBehavior::Wrap => {
                    eligible.last().copied()
                }
                None => eligible.last().copied(),
                Some(_) => None,
            },
            Step::Ignored => unreachable!(),
        };

        let Some(next) = next else {
            self.diagnostics.boundaries += 1;
            return Ok(CompositeChange::Boundary {
                current: self
                    .active_descendant
                    .map_or(CompositeFocusTarget::Root, CompositeFocusTarget::Item),
                command,
            });
        };
        if self.active_descendant == Some(next) {
            self.diagnostics.boundaries += 1;
            return Ok(CompositeChange::Boundary {
                current: CompositeFocusTarget::Item(next),
                command,
            });
        }

        let previous = self
            .active_descendant
            .map_or(CompositeFocusTarget::Root, CompositeFocusTarget::Item);
        self.active_descendant = Some(next);
        self.last_active_descendant = Some(next);
        self.diagnostics.highlight_changes += 1;
        let selection_request = self.navigation_selection_request(next);
        Ok(CompositeChange::Highlighted {
            previous,
            current: next,
            reason: CompositeHighlightReason::Navigation(command),
            selection_request,
        })
    }

    /// Requests selection of the active item without committing the controlled value.
    pub fn request_active_selection(
        &mut self,
        source: ChangeSource,
    ) -> Result<CompositeSelectionRequest<K>, CompositeError<K>> {
        if !self.entered {
            self.diagnostics.failures += 1;
            return Err(CompositeError::NotEntered);
        }
        let Some(key) = self.active_descendant else {
            self.diagnostics.failures += 1;
            return Err(CompositeError::NoActiveDescendant);
        };
        if !self.is_enabled(key) {
            self.diagnostics.failures += 1;
            return Err(CompositeError::ActiveDescendantDisabled(key));
        }

        self.diagnostics.selection_requests += 1;
        Ok(CompositeSelectionRequest { key, source })
    }

    fn navigation_selection_request(&mut self, key: K) -> Option<CompositeSelectionRequest<K>> {
        if self.policy.selection != CompositeSelectionBehavior::FollowsHighlight
            || !self.is_enabled(key)
        {
            return None;
        }
        self.diagnostics.selection_requests += 1;
        Some(CompositeSelectionRequest {
            key,
            source: ChangeSource::Directional,
        })
    }

    fn resolve_step(
        &self,
        command: CompositeNavigationCommand,
        direction: WritingDirection,
    ) -> Step {
        use CompositeNavigationCommand as Command;
        use CompositeOrientation as Orientation;
        match command {
            Command::Previous => Step::Backward,
            Command::Next => Step::Forward,
            Command::Home => Step::First,
            Command::End => Step::Last,
            Command::Up if self.policy.orientation != Orientation::Horizontal => Step::Backward,
            Command::Down if self.policy.orientation != Orientation::Horizontal => Step::Forward,
            Command::Left if self.policy.orientation != Orientation::Vertical => match direction {
                WritingDirection::LeftToRight => Step::Backward,
                WritingDirection::RightToLeft => Step::Forward,
            },
            Command::Right if self.policy.orientation != Orientation::Vertical => match direction {
                WritingDirection::LeftToRight => Step::Forward,
                WritingDirection::RightToLeft => Step::Backward,
            },
            Command::Left | Command::Right | Command::Up | Command::Down => Step::Ignored,
        }
    }

    fn contains(&self, key: K) -> bool {
        self.items.iter().any(|item| item.key == key)
    }

    fn is_enabled(&self, key: K) -> bool {
        self.items
            .iter()
            .find(|item| item.key == key)
            .is_some_and(|item| item.enabled)
    }

    fn is_navigable(&self, key: K) -> bool {
        self.items
            .iter()
            .find(|item| item.key == key)
            .is_some_and(|item| {
                item.enabled || self.policy.disabled_items == DisabledItemPolicy::Include
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct Key {
        slot: u32,
        generation: u32,
    }

    fn key(slot: u32, generation: u32) -> Key {
        Key { slot, generation }
    }

    fn item(slot: u32) -> CompositeItem<Key> {
        CompositeItem {
            key: key(slot, 1),
            enabled: true,
        }
    }

    fn fixture(policy: CompositeNavigationPolicy) -> CompositeStateMachine<Key> {
        let mut machine = CompositeStateMachine::new(policy);
        machine.update_items([item(1), item(2), item(3)]).unwrap();
        machine
    }

    #[test]
    fn entry_prefers_restored_then_selected_then_first_navigable() {
        let mut machine = fixture(CompositeNavigationPolicy::default());
        assert_eq!(
            machine.enter(Some(key(2, 1))).unwrap(),
            CompositeChange::Entered {
                target: CompositeFocusTarget::Item(key(2, 1)),
                reason: CompositeEntryReason::Selected,
            }
        );
        machine.set_active_descendant(key(3, 1)).unwrap();
        machine.leave().unwrap();
        assert_eq!(
            machine.enter(Some(key(1, 1))).unwrap(),
            CompositeChange::Entered {
                target: CompositeFocusTarget::Item(key(3, 1)),
                reason: CompositeEntryReason::Restored,
            }
        );
        assert_eq!(machine.diagnostics().restores, 1);

        let mut fresh = fixture(CompositeNavigationPolicy::default());
        fresh.update_items([item(1), item(3)]).unwrap();
        assert!(matches!(
            fresh.enter(Some(key(2, 1))).unwrap(),
            CompositeChange::Entered {
                target: CompositeFocusTarget::Item(Key { slot: 1, .. }),
                reason: CompositeEntryReason::FirstEnabled,
            }
        ));
    }

    #[test]
    fn navigation_stops_or_wraps_and_home_end_are_absolute() {
        let mut stop = fixture(CompositeNavigationPolicy::default());
        stop.enter(None).unwrap();
        assert!(matches!(
            stop.navigate(
                CompositeNavigationCommand::Previous,
                WritingDirection::LeftToRight
            )
            .unwrap(),
            CompositeChange::Boundary { .. }
        ));
        assert!(matches!(
            stop.navigate(
                CompositeNavigationCommand::End,
                WritingDirection::LeftToRight
            )
            .unwrap(),
            CompositeChange::Highlighted {
                current: Key { slot: 3, .. },
                ..
            }
        ));

        let mut wrap = fixture(CompositeNavigationPolicy {
            edge_behavior: CompositeEdgeBehavior::Wrap,
            ..CompositeNavigationPolicy::default()
        });
        wrap.enter(None).unwrap();
        assert!(matches!(
            wrap.navigate(
                CompositeNavigationCommand::Previous,
                WritingDirection::LeftToRight
            )
            .unwrap(),
            CompositeChange::Highlighted {
                current: Key { slot: 3, .. },
                ..
            }
        ));
    }

    #[test]
    fn horizontal_arrows_mirror_in_rtl_but_logical_commands_do_not() {
        let mut machine = fixture(CompositeNavigationPolicy::default());
        machine.enter(Some(key(2, 1))).unwrap();
        machine
            .navigate(
                CompositeNavigationCommand::Right,
                WritingDirection::RightToLeft,
            )
            .unwrap();
        assert_eq!(machine.active_descendant(), Some(key(1, 1)));
        machine
            .navigate(
                CompositeNavigationCommand::Next,
                WritingDirection::RightToLeft,
            )
            .unwrap();
        assert_eq!(machine.active_descendant(), Some(key(2, 1)));
    }

    #[test]
    fn orientation_ignores_irrelevant_physical_axis() {
        let mut machine = fixture(CompositeNavigationPolicy {
            orientation: CompositeOrientation::Vertical,
            ..CompositeNavigationPolicy::default()
        });
        machine.enter(None).unwrap();
        assert_eq!(
            machine
                .navigate(
                    CompositeNavigationCommand::Right,
                    WritingDirection::LeftToRight
                )
                .unwrap(),
            CompositeChange::Ignored {
                command: CompositeNavigationCommand::Right,
            }
        );
        assert_eq!(machine.active_descendant(), Some(key(1, 1)));
    }

    #[test]
    fn disabled_policy_is_explicit_and_disabled_item_cannot_select() {
        let items = [
            item(1),
            CompositeItem {
                key: key(2, 1),
                enabled: false,
            },
            item(3),
        ];
        let mut skip = CompositeStateMachine::new(CompositeNavigationPolicy::default());
        skip.update_items(items).unwrap();
        skip.enter(None).unwrap();
        skip.navigate(
            CompositeNavigationCommand::Next,
            WritingDirection::LeftToRight,
        )
        .unwrap();
        assert_eq!(skip.active_descendant(), Some(key(3, 1)));

        let mut include = CompositeStateMachine::new(CompositeNavigationPolicy {
            disabled_items: DisabledItemPolicy::Include,
            selection: CompositeSelectionBehavior::FollowsHighlight,
            ..CompositeNavigationPolicy::default()
        });
        include.update_items(items).unwrap();
        include.enter(None).unwrap();
        let change = include
            .navigate(
                CompositeNavigationCommand::Next,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert!(matches!(
            change,
            CompositeChange::Highlighted {
                current: Key { slot: 2, .. },
                selection_request: None,
                ..
            }
        ));
        assert_eq!(
            include.request_active_selection(ChangeSource::Keyboard),
            Err(CompositeError::ActiveDescendantDisabled(key(2, 1)))
        );
    }

    #[test]
    fn selection_following_emits_a_request_without_committing_selection() {
        let mut machine = fixture(CompositeNavigationPolicy {
            selection: CompositeSelectionBehavior::FollowsHighlight,
            ..CompositeNavigationPolicy::default()
        });
        machine.enter(Some(key(1, 1))).unwrap();
        assert!(matches!(
            machine
                .navigate(
                    CompositeNavigationCommand::Next,
                    WritingDirection::LeftToRight
                )
                .unwrap(),
            CompositeChange::Highlighted {
                current: Key { slot: 2, .. },
                selection_request: Some(CompositeSelectionRequest {
                    key: Key { slot: 2, .. },
                    source: ChangeSource::Directional,
                }),
                ..
            }
        ));
        assert_eq!(machine.diagnostics().selection_requests, 1);
    }

    #[test]
    fn explicit_selection_request_preserves_source() {
        let mut machine = fixture(CompositeNavigationPolicy::default());
        machine.enter(None).unwrap();
        assert_eq!(
            machine
                .request_active_selection(ChangeSource::Accessibility)
                .unwrap(),
            CompositeSelectionRequest {
                key: key(1, 1),
                source: ChangeSource::Accessibility,
            }
        );
        assert_eq!(machine.active_descendant(), Some(key(1, 1)));
    }

    #[test]
    fn removal_prefers_old_surviving_successor_then_predecessor() {
        let mut machine = fixture(CompositeNavigationPolicy::default());
        machine.enter(Some(key(2, 1))).unwrap();
        assert!(matches!(
            machine.update_items([item(1), item(3)]).unwrap(),
            CompositeChange::Highlighted {
                current: Key { slot: 3, .. },
                reason: CompositeHighlightReason::ItemsChanged,
                ..
            }
        ));
        assert!(matches!(
            machine.update_items([item(1)]).unwrap(),
            CompositeChange::Highlighted {
                current: Key { slot: 1, .. },
                ..
            }
        ));
    }

    #[test]
    fn recycled_generation_is_not_an_implicit_removal_target() {
        let mut machine = CompositeStateMachine::new(CompositeNavigationPolicy::default());
        machine
            .update_items([CompositeItem {
                key: key(4, 1),
                enabled: true,
            }])
            .unwrap();
        machine.enter(None).unwrap();
        assert_eq!(
            machine
                .update_items([CompositeItem {
                    key: key(4, 2),
                    enabled: true,
                }])
                .unwrap(),
            CompositeChange::Rooted {
                previous: key(4, 1),
                reason: CompositeHighlightReason::ItemsChanged,
            }
        );
        assert_eq!(machine.active_target(), Some(CompositeFocusTarget::Root));
    }

    #[test]
    fn duplicate_update_is_atomic() {
        let mut machine = fixture(CompositeNavigationPolicy::default());
        machine.enter(None).unwrap();
        assert_eq!(
            machine.update_items([item(8), item(8)]),
            Err(CompositeError::DuplicateKey(key(8, 1)))
        );
        assert_eq!(machine.active_descendant(), Some(key(1, 1)));
        machine
            .navigate(
                CompositeNavigationCommand::Next,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(machine.active_descendant(), Some(key(2, 1)));
    }

    #[test]
    fn removal_while_outside_clears_stale_history_before_reentry() {
        let mut machine = fixture(CompositeNavigationPolicy::default());
        machine.enter(Some(key(2, 1))).unwrap();
        machine.leave().unwrap();
        machine.update_items([item(1), item(3)]).unwrap();
        assert_eq!(machine.last_active_descendant(), None);
        assert!(matches!(
            machine.enter(Some(key(3, 1))).unwrap(),
            CompositeChange::Entered {
                target: CompositeFocusTarget::Item(Key { slot: 3, .. }),
                reason: CompositeEntryReason::Selected,
            }
        ));
    }

    #[test]
    fn no_navigable_item_leaves_focus_on_composite_root() {
        let mut machine = CompositeStateMachine::new(CompositeNavigationPolicy::default());
        machine
            .update_items([CompositeItem {
                key: key(1, 1),
                enabled: false,
            }])
            .unwrap();
        assert_eq!(
            machine.enter(Some(key(1, 1))).unwrap(),
            CompositeChange::Entered {
                target: CompositeFocusTarget::Root,
                reason: CompositeEntryReason::NoNavigableItem,
            }
        );
        assert!(matches!(
            machine
                .navigate(
                    CompositeNavigationCommand::Next,
                    WritingDirection::LeftToRight
                )
                .unwrap(),
            CompositeChange::Boundary {
                current: CompositeFocusTarget::Root,
                ..
            }
        ));
    }

    #[test]
    fn entry_fallback_uses_first_enabled_even_when_disabled_items_are_discoverable() {
        let mut machine = CompositeStateMachine::new(CompositeNavigationPolicy {
            disabled_items: DisabledItemPolicy::Include,
            ..CompositeNavigationPolicy::default()
        });
        machine
            .update_items([
                CompositeItem {
                    key: key(1, 1),
                    enabled: false,
                },
                item(2),
            ])
            .unwrap();
        assert_eq!(
            machine.enter(None).unwrap(),
            CompositeChange::Entered {
                target: CompositeFocusTarget::Item(key(2, 1)),
                reason: CompositeEntryReason::FirstEnabled,
            }
        );
    }
}

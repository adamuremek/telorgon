//! Platform-neutral clipboard capabilities, snapshots, and request admission.
//!
//! This module models clipboard metadata only. An offer describes formats and size hints through
//! the shared data-transfer vocabulary; it never contains clipboard bytes. Reading one format is
//! therefore a separate asynchronous data-transfer operation. Implementations of
//! [`ClipboardService`] belong to a host or adapter and may admit publish/clear work, but this
//! module invokes no native API, owns no queue or callback, and performs no blocking I/O.

use std::error::Error;
use std::fmt;
use std::num::{NonZeroU32, NonZeroU64};
use std::rc::Rc;
use std::sync::Arc;

use crate::platform::{
    CapabilityLimit, ExecutionRequirement, PermissionState, PlatformError, RequestAdmission,
    Support, UnavailableReason, UserGestureRequirement,
};

use super::ServiceKey;
use super::data_transfer::{
    DataFormat, DataOfferDescriptor, DataSourceKind, MAX_DATA_FORMATS_PER_OFFER,
    MAX_DATA_READ_BYTES,
};

/// Maximum number of distinct formats one clipboard capability may advertise.
pub const MAX_CLIPBOARD_CAPABILITY_FORMATS: usize = MAX_DATA_FORMATS_PER_OFFER;

/// A separately owned platform clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardKind {
    /// The ordinary copy/paste clipboard.
    System,
    /// The selection/primary clipboard available on platforms that expose one.
    Selection,
}

/// Monotonic revision of snapshots for one [`ClipboardKind`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipboardRevision(NonZeroU64);

impl ClipboardRevision {
    /// First valid revision in deterministic fixtures or a newly initialized adapter.
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    /// Wraps a host-issued nonzero revision.
    pub const fn new(revision: NonZeroU64) -> Self {
        Self(revision)
    }

    /// Wraps a raw revision, rejecting the reserved zero value.
    pub const fn from_raw(revision: u64) -> Option<Self> {
        match NonZeroU64::new(revision) {
            Some(revision) => Some(Self(revision)),
            None => None,
        }
    }

    /// Returns the host-local sequence value.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Advances this revision, returning `None` on exhaustion.
    pub const fn checked_next(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(next) => Self::from_raw(next),
            None => None,
        }
    }
}

impl fmt::Display for ClipboardRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Identity of one coherent clipboard snapshot.
///
/// Revisions are scoped to a clipboard kind, so retaining the kind prevents a system snapshot
/// from being cited as the expected state of the selection clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipboardSnapshotId {
    clipboard: ClipboardKind,
    revision: ClipboardRevision,
}

impl ClipboardSnapshotId {
    /// Creates an identity from its explicit clipboard scope and host-issued revision.
    pub const fn new(clipboard: ClipboardKind, revision: ClipboardRevision) -> Self {
        Self {
            clipboard,
            revision,
        }
    }

    /// Returns the clipboard whose history owns the revision.
    pub const fn clipboard(self) -> ClipboardKind {
        self.clipboard
    }

    /// Returns the monotonic revision within that clipboard's history.
    pub const fn revision(self) -> ClipboardRevision {
        self.revision
    }
}

/// Failure to construct coherent clipboard snapshot or change metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardSnapshotError {
    /// A current offer was not issued as a clipboard offer.
    OfferSourceIsNotClipboard { source: DataSourceKind },
    /// A change joined identities from different clipboard histories.
    ClipboardMismatch {
        previous: ClipboardKind,
        current: ClipboardKind,
    },
    /// A change did not advance beyond the previously observed revision.
    RevisionDidNotAdvance {
        previous: ClipboardRevision,
        current: ClipboardRevision,
    },
}

impl fmt::Display for ClipboardSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OfferSourceIsNotClipboard { .. } => {
                formatter.write_str("clipboard snapshot offer has a non-clipboard source")
            }
            Self::ClipboardMismatch { .. } => {
                formatter.write_str("clipboard change joins different clipboard histories")
            }
            Self::RevisionDidNotAdvance { .. } => {
                formatter.write_str("clipboard change revision did not advance")
            }
        }
    }
}

impl Error for ClipboardSnapshotError {}

/// One immutable publication of the currently observed clipboard offer.
///
/// `None` means the observed clipboard was empty. The optional descriptor contains metadata only;
/// reading bytes requires a separately admitted bounded data-transfer request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardSnapshot {
    id: ClipboardSnapshotId,
    current_offer: Option<DataOfferDescriptor>,
}

impl ClipboardSnapshot {
    /// Constructs a coherent snapshot, rejecting drag/drop or share offers.
    pub fn new(
        id: ClipboardSnapshotId,
        current_offer: Option<DataOfferDescriptor>,
    ) -> Result<Self, ClipboardSnapshotError> {
        if let Some(offer) = &current_offer
            && offer.source() != DataSourceKind::Clipboard
        {
            return Err(ClipboardSnapshotError::OfferSourceIsNotClipboard {
                source: offer.source(),
            });
        }

        Ok(Self { id, current_offer })
    }

    /// Returns this publication's clipboard-scoped identity.
    pub const fn id(&self) -> ClipboardSnapshotId {
        self.id
    }

    /// Returns the separately owned clipboard represented by this snapshot.
    pub const fn clipboard(&self) -> ClipboardKind {
        self.id.clipboard()
    }

    /// Returns the current metadata-only offer, or `None` when observed empty.
    pub const fn current_offer(&self) -> Option<&DataOfferDescriptor> {
        self.current_offer.as_ref()
    }
}

/// One change notification carrying the new coherent snapshot.
///
/// A first publication may omit `previous`. Otherwise the current revision must strictly advance
/// in the same clipboard history. The surrounding platform event supplies ordering and time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardChange {
    previous: Option<ClipboardSnapshotId>,
    current: ClipboardSnapshot,
}

impl ClipboardChange {
    /// Validates and constructs a clipboard change notification.
    pub fn new(
        previous: Option<ClipboardSnapshotId>,
        current: ClipboardSnapshot,
    ) -> Result<Self, ClipboardSnapshotError> {
        if let Some(previous) = previous {
            let current_id = current.id();
            if previous.clipboard() != current_id.clipboard() {
                return Err(ClipboardSnapshotError::ClipboardMismatch {
                    previous: previous.clipboard(),
                    current: current_id.clipboard(),
                });
            }
            if current_id.revision() <= previous.revision() {
                return Err(ClipboardSnapshotError::RevisionDidNotAdvance {
                    previous: previous.revision(),
                    current: current_id.revision(),
                });
            }
        }

        Ok(Self { previous, current })
    }

    /// Returns the previously observed identity when this is not an initial publication.
    pub const fn previous(&self) -> Option<ClipboardSnapshotId> {
        self.previous
    }

    /// Returns the newly observed coherent snapshot.
    pub const fn current(&self) -> &ClipboardSnapshot {
        &self.current
    }
}

/// Operations advertised for one clipboard kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipboardOperations {
    snapshot: bool,
    publish: bool,
    clear: bool,
    change_notifications: bool,
}

impl ClipboardOperations {
    /// Creates an explicit operation set.
    pub const fn new(
        snapshot: bool,
        publish: bool,
        clear: bool,
        change_notifications: bool,
    ) -> Self {
        Self {
            snapshot,
            publish,
            clear,
            change_notifications,
        }
    }

    /// Whether current metadata snapshots can be queried.
    pub const fn supports_snapshot(self) -> bool {
        self.snapshot
    }

    /// Whether format-aware offers can be published.
    pub const fn supports_publish(self) -> bool {
        self.publish
    }

    /// Whether current ownership/content can be explicitly cleared.
    pub const fn supports_clear(self) -> bool {
        self.clear
    }

    /// Whether the adapter publishes revisioned ownership/content changes.
    pub const fn supports_change_notifications(self) -> bool {
        self.change_notifications
    }
}

/// Declared limits for publish and read coordination on one clipboard kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClipboardLimits {
    formats_per_offer: CapabilityLimit<NonZeroU32>,
    bytes_per_format: CapabilityLimit<NonZeroU64>,
}

impl ClipboardLimits {
    /// Validates explicit format-count and per-format byte limit facts.
    pub fn new(
        formats_per_offer: CapabilityLimit<NonZeroU32>,
        bytes_per_format: CapabilityLimit<NonZeroU64>,
    ) -> Result<Self, ClipboardLimitError> {
        if let CapabilityLimit::Bounded(limit) = formats_per_offer
            && limit.get() as usize > MAX_DATA_FORMATS_PER_OFFER
        {
            return Err(ClipboardLimitError::TooManyFormats);
        }
        if let CapabilityLimit::Bounded(limit) = bytes_per_format
            && limit.get() > MAX_DATA_READ_BYTES
        {
            return Err(ClipboardLimitError::BytesPerFormatTooLarge);
        }

        Ok(Self {
            formats_per_offer,
            bytes_per_format,
        })
    }

    /// Returns the maximum offered-format count when the adapter reports one.
    pub const fn formats_per_offer(&self) -> CapabilityLimit<&NonZeroU32> {
        self.formats_per_offer.as_ref()
    }

    /// Returns the maximum byte count for reading or producing one format when reported.
    pub const fn bytes_per_format(&self) -> CapabilityLimit<&NonZeroU64> {
        self.bytes_per_format.as_ref()
    }
}

/// Invalid service limit metadata that exceeds the shared transfer ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardLimitError {
    /// A published offer could not be represented by the shared offer descriptor.
    TooManyFormats,
    /// A per-format byte limit exceeded the maximum admitted shared data read.
    BytesPerFormatTooLarge,
}

impl fmt::Display for ClipboardLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyFormats => "clipboard format limit exceeds the shared offer bound",
            Self::BytesPerFormatTooLarge => {
                "clipboard byte limit exceeds the shared data-read bound"
            }
        })
    }
}

impl Error for ClipboardLimitError {}

/// Failure to validate an available clipboard capability descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardCapabilityError {
    /// The advertised format list exceeded the neutral metadata bound.
    TooManyFormats { supplied: usize, maximum: usize },
    /// Publishing was advertised without any producible format.
    PublishWithoutFormats,
}

impl fmt::Display for ClipboardCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyFormats { supplied, maximum } => write!(
                formatter,
                "clipboard capability advertises {supplied} formats; maximum is {maximum}"
            ),
            Self::PublishWithoutFormats => {
                formatter.write_str("clipboard publish capability requires at least one format")
            }
        }
    }
}

impl Error for ClipboardCapabilityError {}

/// Available capability facts for one clipboard kind.
///
/// Read/snapshot and write permissions remain separate because platforms commonly apply different
/// policy to those directions. The supported format list is bounded and deduplicated without
/// changing host preference order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCapability {
    operations: ClipboardOperations,
    formats: Arc<[DataFormat]>,
    limits: ClipboardLimits,
    read_permission: PermissionState,
    write_permission: PermissionState,
    execution: ExecutionRequirement,
    user_gesture: UserGestureRequirement,
}

impl ClipboardCapability {
    /// Validates and constructs capability facts for one clipboard kind.
    pub fn new(
        operations: ClipboardOperations,
        formats: Vec<DataFormat>,
        limits: ClipboardLimits,
        read_permission: PermissionState,
        write_permission: PermissionState,
        execution: ExecutionRequirement,
        user_gesture: UserGestureRequirement,
    ) -> Result<Self, ClipboardCapabilityError> {
        if formats.len() > MAX_CLIPBOARD_CAPABILITY_FORMATS {
            return Err(ClipboardCapabilityError::TooManyFormats {
                supplied: formats.len(),
                maximum: MAX_CLIPBOARD_CAPABILITY_FORMATS,
            });
        }

        let mut distinct = Vec::with_capacity(formats.len());
        for format in formats {
            if !distinct.contains(&format) {
                distinct.push(format);
            }
        }
        if operations.supports_publish() && distinct.is_empty() {
            return Err(ClipboardCapabilityError::PublishWithoutFormats);
        }

        Ok(Self {
            operations,
            formats: distinct.into(),
            limits,
            read_permission,
            write_permission,
            execution,
            user_gesture,
        })
    }

    /// Returns the advertised operation set.
    pub const fn operations(&self) -> ClipboardOperations {
        self.operations
    }

    /// Returns supported publish/read formats in host preference order.
    pub fn formats(&self) -> &[DataFormat] {
        &self.formats
    }

    /// Returns whether this exact format is advertised.
    pub fn supports_format(&self, format: &DataFormat) -> bool {
        self.formats.contains(format)
    }

    /// Returns format-count and per-format byte limits.
    pub const fn limits(&self) -> &ClipboardLimits {
        &self.limits
    }

    /// Returns permission for observing offer metadata and requesting data reads.
    pub const fn read_permission(&self) -> PermissionState {
        self.read_permission
    }

    /// Returns permission for publish and clear operations.
    pub const fn write_permission(&self) -> PermissionState {
        self.write_permission
    }

    /// Returns the host context on which the adapter executes clipboard work.
    pub const fn execution(&self) -> ExecutionRequirement {
        self.execution
    }

    /// Returns whether mutation requires a recent host-validated user gesture.
    pub const fn user_gesture(&self) -> UserGestureRequirement {
        self.user_gesture
    }
}

/// Capability discovery for both independently supported clipboard kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCapabilities {
    system: Support<ClipboardCapability>,
    selection: Support<ClipboardCapability>,
}

impl ClipboardCapabilities {
    /// Creates explicit support facts without constructing a fallback clipboard.
    pub const fn new(
        system: Support<ClipboardCapability>,
        selection: Support<ClipboardCapability>,
    ) -> Self {
        Self { system, selection }
    }

    /// Returns support for the requested independently owned clipboard.
    pub const fn for_clipboard(&self, clipboard: ClipboardKind) -> Support<&ClipboardCapability> {
        match clipboard {
            ClipboardKind::System => self.system.as_ref(),
            ClipboardKind::Selection => self.selection.as_ref(),
        }
    }
}

/// Failure to validate an immutable clipboard mutation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardRequestError {
    /// A publish request cited a drag/drop or share offer.
    OfferSourceIsNotClipboard { source: DataSourceKind },
    /// Optimistic concurrency cited a snapshot from the other clipboard.
    ExpectedSnapshotClipboardMismatch {
        requested: ClipboardKind,
        expected: ClipboardKind,
    },
}

impl fmt::Display for ClipboardRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OfferSourceIsNotClipboard { .. } => {
                formatter.write_str("clipboard publish request has a non-clipboard source")
            }
            Self::ExpectedSnapshotClipboardMismatch { .. } => {
                formatter.write_str("clipboard request cites a snapshot from a different clipboard")
            }
        }
    }
}

impl Error for ClipboardRequestError {}

/// Immutable request to publish one metadata-described, lazily readable clipboard offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardPublishRequest {
    clipboard: ClipboardKind,
    offer: DataOfferDescriptor,
    expected: Option<ClipboardSnapshotId>,
}

impl ClipboardPublishRequest {
    /// Validates offer provenance and optional optimistic-concurrency scope.
    pub fn new(
        clipboard: ClipboardKind,
        offer: DataOfferDescriptor,
        expected: Option<ClipboardSnapshotId>,
    ) -> Result<Self, ClipboardRequestError> {
        if offer.source() != DataSourceKind::Clipboard {
            return Err(ClipboardRequestError::OfferSourceIsNotClipboard {
                source: offer.source(),
            });
        }
        validate_expected_clipboard(clipboard, expected)?;
        Ok(Self {
            clipboard,
            offer,
            expected,
        })
    }

    /// Returns the clipboard to receive the offer.
    pub const fn clipboard(&self) -> ClipboardKind {
        self.clipboard
    }

    /// Returns the payload-free offer descriptor.
    pub const fn offer(&self) -> &DataOfferDescriptor {
        &self.offer
    }

    /// Returns the snapshot that must still be current, when requested.
    pub const fn expected(&self) -> Option<ClipboardSnapshotId> {
        self.expected
    }
}

/// Immutable request to clear one clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipboardClearRequest {
    clipboard: ClipboardKind,
    expected: Option<ClipboardSnapshotId>,
}

impl ClipboardClearRequest {
    /// Validates optional optimistic-concurrency scope.
    pub fn new(
        clipboard: ClipboardKind,
        expected: Option<ClipboardSnapshotId>,
    ) -> Result<Self, ClipboardRequestError> {
        validate_expected_clipboard(clipboard, expected)?;
        Ok(Self {
            clipboard,
            expected,
        })
    }

    /// Returns the clipboard to clear.
    pub const fn clipboard(self) -> ClipboardKind {
        self.clipboard
    }

    /// Returns the snapshot that must still be current, when requested.
    pub const fn expected(self) -> Option<ClipboardSnapshotId> {
        self.expected
    }
}

fn validate_expected_clipboard(
    requested: ClipboardKind,
    expected: Option<ClipboardSnapshotId>,
) -> Result<(), ClipboardRequestError> {
    if let Some(expected) = expected
        && expected.clipboard() != requested
    {
        return Err(ClipboardRequestError::ExpectedSnapshotClipboardMismatch {
            requested,
            expected: expected.clipboard(),
        });
    }
    Ok(())
}

/// Applied result of publishing an offer.
///
/// This does not invent a new clipboard revision. A subsequent [`ClipboardSnapshot`] remains the
/// observed truth about current ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipboardPublishApplied {
    clipboard: ClipboardKind,
    offer: crate::platform::DataOfferId,
}

impl ClipboardPublishApplied {
    /// Derives a payload-free receipt from the admitted publish request.
    pub const fn from_request(request: &ClipboardPublishRequest) -> Self {
        Self {
            clipboard: request.clipboard,
            offer: request.offer.id(),
        }
    }

    /// Returns the clipboard targeted by the completed request.
    pub const fn clipboard(self) -> ClipboardKind {
        self.clipboard
    }

    /// Returns the descriptor identity that was published.
    pub const fn offer(self) -> crate::platform::DataOfferId {
        self.offer
    }
}

/// Applied result of clearing a clipboard.
///
/// This records completion only; the next revisioned snapshot reports observed emptiness or a
/// replacement offer from another owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipboardClearApplied {
    clipboard: ClipboardKind,
}

impl ClipboardClearApplied {
    /// Derives a receipt from the admitted clear request.
    pub const fn from_request(request: ClipboardClearRequest) -> Self {
        Self {
            clipboard: request.clipboard,
        }
    }

    /// Returns the clipboard targeted by the completed request.
    pub const fn clipboard(self) -> ClipboardKind {
        self.clipboard
    }
}

/// Result of querying current clipboard metadata without reading clipboard data.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use = "clipboard snapshot absence and failure must be handled explicitly"]
pub enum ClipboardSnapshotStatus {
    /// A current coherent snapshot was available.
    Current(ClipboardSnapshot),
    /// Snapshot observation is not currently supported or available.
    Unavailable(UnavailableReason),
    /// Snapshot observation failed after a service was available.
    Failed(PlatformError),
}

impl ClipboardSnapshotStatus {
    /// Borrows the current snapshot when observation succeeded.
    pub const fn current(&self) -> Option<&ClipboardSnapshot> {
        match self {
            Self::Current(snapshot) => Some(snapshot),
            Self::Unavailable(_) | Self::Failed(_) => None,
        }
    }

    /// Returns the explicit support-absence reason, if any.
    pub const fn unavailable_reason(&self) -> Option<UnavailableReason> {
        match self {
            Self::Unavailable(reason) => Some(*reason),
            Self::Current(_) | Self::Failed(_) => None,
        }
    }

    /// Returns the structured observation failure, if any.
    pub const fn failure(&self) -> Option<PlatformError> {
        match self {
            Self::Failed(error) => Some(*error),
            Self::Current(_) | Self::Unavailable(_) => None,
        }
    }
}

/// Immediate failure to admit an otherwise well-formed clipboard request.
///
/// Permission denial, unsupported execution, cancellation, staleness after admission, and native
/// failure remain distinct terminal [`crate::platform::RequestOutcome`] variants rather than this error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardAdmissionError {
    /// The independently selected clipboard is no longer available.
    ClipboardUnavailable { clipboard: ClipboardKind },
    /// Capability facts changed between query and admission.
    CapabilityChanged { clipboard: ClipboardKind },
    /// The offer contains more representations than the current service limit.
    FormatCountExceedsCapability {
        clipboard: ClipboardKind,
        supplied: usize,
        maximum: NonZeroU32,
    },
    /// One exact offered representation is not supported by the current capability.
    FormatNotSupported {
        clipboard: ClipboardKind,
        format_index: usize,
    },
    /// One declared size hint exceeds the current service limit.
    SizeHintExceedsCapability {
        clipboard: ClipboardKind,
        format_index: usize,
        hinted_bytes: u64,
        maximum_bytes: NonZeroU64,
    },
}

impl fmt::Display for ClipboardAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClipboardUnavailable { clipboard } => {
                write!(formatter, "{clipboard:?} clipboard is unavailable")
            }
            Self::CapabilityChanged { clipboard } => {
                write!(
                    formatter,
                    "{clipboard:?} clipboard capability changed before admission"
                )
            }
            Self::FormatCountExceedsCapability {
                clipboard,
                supplied,
                maximum,
            } => write!(
                formatter,
                "{clipboard:?} clipboard offer contains {supplied} formats; capability maximum is {maximum}"
            ),
            Self::FormatNotSupported {
                clipboard,
                format_index,
            } => write!(
                formatter,
                "{clipboard:?} clipboard offer format at index {format_index} is unsupported"
            ),
            Self::SizeHintExceedsCapability {
                clipboard,
                format_index,
                hinted_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "{clipboard:?} clipboard offer size hint {hinted_bytes} at index {format_index} exceeds capability maximum {maximum_bytes}"
            ),
        }
    }
}

impl Error for ClipboardAdmissionError {}

/// Admission result for one operation-specific clipboard applied type.
///
/// Immediate rejection has no request identity. Success yields the existing linear admitted token
/// whose terminal completion remains separate from this service interface.
pub type ClipboardRequestAdmission<T> = RequestAdmission<T, ClipboardAdmissionError>;

/// Host- or adapter-supplied clipboard admission boundary.
///
/// Snapshot queries return metadata only. Admitting a request returns a linear completion token;
/// it does not synchronously produce/read clipboard bytes or imply that native work completed.
/// Implementations decide how completions re-enter their host using infrastructure outside this
/// trait.
pub trait ClipboardService {
    /// Returns explicit capability support for one independently owned clipboard.
    fn capability(&self, clipboard: ClipboardKind) -> Support<ClipboardCapability>;

    /// Returns the current metadata snapshot or explicit absence/failure.
    fn current_snapshot(&self, clipboard: ClipboardKind) -> ClipboardSnapshotStatus;

    /// Validates and admits publication without blocking for its completion.
    fn publish(
        &self,
        request: ClipboardPublishRequest,
    ) -> ClipboardRequestAdmission<ClipboardPublishApplied>;

    /// Validates and admits clearing without blocking for its completion.
    fn clear(
        &self,
        request: ClipboardClearRequest,
    ) -> ClipboardRequestAdmission<ClipboardClearApplied>;
}

/// Type-level registry key for an owner-thread clipboard service handle.
pub enum ClipboardServiceKey {}

impl ServiceKey for ClipboardServiceKey {
    type Handle = Rc<dyn ClipboardService>;
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use crate::platform::{
        DataOfferId, ExecutionRequirement, PlatformErrorKind, RequestId, RequestOutcome,
        ServiceLookup, ServiceRegistry,
    };

    use super::super::data_transfer::{SizeHint, TrustLevel};
    use super::*;

    fn offer(source: DataSourceKind) -> DataOfferDescriptor {
        DataOfferDescriptor::new(
            DataOfferId::from_raw(7, 2).unwrap(),
            vec![DataFormat::mime("text/plain;charset=utf-8").unwrap()],
            source,
            TrustLevel::Trusted,
            vec![SizeHint::AtMost(32)],
        )
        .unwrap()
    }

    fn capabilities() -> ClipboardCapabilities {
        let format = DataFormat::mime("text/plain;charset=utf-8").unwrap();
        let system = ClipboardCapability::new(
            ClipboardOperations::new(true, true, true, true),
            vec![format.clone(), format],
            ClipboardLimits::default(),
            PermissionState::Granted,
            PermissionState::PromptRequired,
            ExecutionRequirement::HostEventLoop,
            UserGestureRequirement::RecentRequired,
        )
        .unwrap();
        ClipboardCapabilities::new(
            Support::Available(system),
            Support::Unavailable(UnavailableReason::UnsupportedByPlatform),
        )
    }

    #[test]
    fn snapshot_and_change_keep_clipboard_identity_and_monotonic_revision() {
        let first_id = ClipboardSnapshotId::new(ClipboardKind::System, ClipboardRevision::INITIAL);
        let first =
            ClipboardSnapshot::new(first_id, Some(offer(DataSourceKind::Clipboard))).unwrap();
        assert_eq!(first.clipboard(), ClipboardKind::System);
        assert_eq!(first.current_offer().unwrap().formats().len(), 1);

        let second_id = ClipboardSnapshotId::new(
            ClipboardKind::System,
            ClipboardRevision::from_raw(2).unwrap(),
        );
        let changed = ClipboardChange::new(
            Some(first_id),
            ClipboardSnapshot::new(second_id, None).unwrap(),
        )
        .unwrap();
        assert_eq!(changed.previous(), Some(first_id));
        assert!(changed.current().current_offer().is_none());

        let stale = ClipboardSnapshot::new(first_id, None).unwrap();
        assert!(matches!(
            ClipboardChange::new(Some(first_id), stale),
            Err(ClipboardSnapshotError::RevisionDidNotAdvance { .. })
        ));
        assert!(matches!(
            ClipboardChange::new(
                Some(first_id),
                ClipboardSnapshot::new(
                    ClipboardSnapshotId::new(ClipboardKind::Selection, ClipboardRevision::INITIAL),
                    None,
                )
                .unwrap(),
            ),
            Err(ClipboardSnapshotError::ClipboardMismatch { .. })
        ));
    }

    #[test]
    fn snapshot_and_request_reject_non_clipboard_offers_and_cross_kind_expectations() {
        let drag = offer(DataSourceKind::DragAndDrop);
        assert!(matches!(
            ClipboardSnapshot::new(
                ClipboardSnapshotId::new(ClipboardKind::System, ClipboardRevision::INITIAL),
                Some(drag.clone()),
            ),
            Err(ClipboardSnapshotError::OfferSourceIsNotClipboard { .. })
        ));
        assert!(matches!(
            ClipboardPublishRequest::new(ClipboardKind::System, drag, None),
            Err(ClipboardRequestError::OfferSourceIsNotClipboard { .. })
        ));

        let selection = ClipboardSnapshotId::new(
            ClipboardKind::Selection,
            ClipboardRevision::from_raw(9).unwrap(),
        );
        assert!(matches!(
            ClipboardClearRequest::new(ClipboardKind::System, Some(selection)),
            Err(ClipboardRequestError::ExpectedSnapshotClipboardMismatch { .. })
        ));
    }

    #[test]
    fn capability_keeps_permissions_targets_formats_and_bounds_explicit() {
        let capabilities = capabilities();
        let Support::Available(system) = capabilities.for_clipboard(ClipboardKind::System) else {
            panic!("system clipboard must be available")
        };
        assert!(system.operations().supports_snapshot());
        assert!(system.operations().supports_change_notifications());
        assert_eq!(system.formats().len(), 1);
        assert_eq!(system.read_permission(), PermissionState::Granted);
        assert_eq!(system.write_permission(), PermissionState::PromptRequired);
        assert_eq!(system.execution(), ExecutionRequirement::HostEventLoop);
        assert!(system.user_gesture().is_required());
        assert_eq!(
            capabilities
                .for_clipboard(ClipboardKind::Selection)
                .unavailable_reason(),
            Some(UnavailableReason::UnsupportedByPlatform)
        );

        let formats = (0..=MAX_CLIPBOARD_CAPABILITY_FORMATS)
            .map(|index| DataFormat::mime(&format!("application/x-telorgon-{index}")).unwrap())
            .collect();
        assert!(matches!(
            ClipboardCapability::new(
                ClipboardOperations::new(false, true, false, false),
                formats,
                ClipboardLimits::default(),
                PermissionState::Unknown,
                PermissionState::Unknown,
                ExecutionRequirement::HostExecutor,
                UserGestureRequirement::NotRequired,
            ),
            Err(ClipboardCapabilityError::TooManyFormats { .. })
        ));
        assert_eq!(
            ClipboardLimits::new(
                CapabilityLimit::Bounded(
                    NonZeroU32::new((MAX_DATA_FORMATS_PER_OFFER + 1) as u32).unwrap(),
                ),
                CapabilityLimit::Unspecified,
            ),
            Err(ClipboardLimitError::TooManyFormats)
        );
        assert_eq!(
            ClipboardLimits::new(
                CapabilityLimit::Unspecified,
                CapabilityLimit::Bounded(NonZeroU64::new(MAX_DATA_READ_BYTES + 1).unwrap()),
            ),
            Err(ClipboardLimitError::BytesPerFormatTooLarge)
        );
    }

    struct FixtureClipboard {
        capabilities: ClipboardCapabilities,
        snapshot: ClipboardSnapshot,
    }

    impl ClipboardService for FixtureClipboard {
        fn capability(&self, clipboard: ClipboardKind) -> Support<ClipboardCapability> {
            self.capabilities.for_clipboard(clipboard).map(Clone::clone)
        }

        fn current_snapshot(&self, clipboard: ClipboardKind) -> ClipboardSnapshotStatus {
            if clipboard == ClipboardKind::System {
                ClipboardSnapshotStatus::Current(self.snapshot.clone())
            } else {
                ClipboardSnapshotStatus::Unavailable(UnavailableReason::UnsupportedByPlatform)
            }
        }

        fn publish(
            &self,
            request: ClipboardPublishRequest,
        ) -> ClipboardRequestAdmission<ClipboardPublishApplied> {
            if request.clipboard() == ClipboardKind::Selection {
                return Err(ClipboardAdmissionError::ClipboardUnavailable {
                    clipboard: ClipboardKind::Selection,
                });
            }
            Ok(crate::platform::AdmittedRequest::new(
                RequestId::from_raw(44).unwrap(),
            ))
        }

        fn clear(
            &self,
            request: ClipboardClearRequest,
        ) -> ClipboardRequestAdmission<ClipboardClearApplied> {
            if request.clipboard() == ClipboardKind::Selection {
                return Err(ClipboardAdmissionError::ClipboardUnavailable {
                    clipboard: ClipboardKind::Selection,
                });
            }
            Ok(crate::platform::AdmittedRequest::new(
                RequestId::from_raw(45).unwrap(),
            ))
        }
    }

    #[test]
    fn registry_handle_is_object_safe_and_admission_stays_separate_from_completion() {
        let snapshot = ClipboardSnapshot::new(
            ClipboardSnapshotId::new(ClipboardKind::System, ClipboardRevision::INITIAL),
            Some(offer(DataSourceKind::Clipboard)),
        )
        .unwrap();
        let service: Rc<dyn ClipboardService> = Rc::new(FixtureClipboard {
            capabilities: capabilities(),
            snapshot,
        });
        let mut registry = ServiceRegistry::new();
        assert!(
            registry
                .register::<ClipboardServiceKey>(service)
                .is_registered()
        );

        let ServiceLookup::Available(service) = registry.lookup::<ClipboardServiceKey>() else {
            panic!("registered clipboard must be available")
        };
        assert!(
            service
                .current_snapshot(ClipboardKind::System)
                .current()
                .is_some()
        );
        assert!(matches!(
            service.capability(ClipboardKind::System),
            Support::Available(_)
        ));
        let request = ClipboardClearRequest::new(ClipboardKind::System, None).unwrap();
        let applied = ClipboardClearApplied::from_request(request);
        let admitted = service.clear(request).unwrap();
        assert_eq!(admitted.request_id(), RequestId::from_raw(45).unwrap());
        let completion = admitted.complete(RequestOutcome::Applied(applied));
        assert!(completion.outcome().is_applied());
    }

    #[test]
    fn status_distinguishes_absence_and_failure_and_debug_has_no_content_payload() {
        let unavailable =
            ClipboardSnapshotStatus::Unavailable(UnavailableReason::ExecutionContextUnavailable);
        assert_eq!(
            unavailable.unavailable_reason(),
            Some(UnavailableReason::ExecutionContextUnavailable)
        );

        let error = PlatformError::new(
            PlatformErrorKind::TransportFailure,
            "clipboard snapshot observation",
        );
        let failed = ClipboardSnapshotStatus::Failed(error);
        assert_eq!(failed.failure(), Some(error));

        let request = ClipboardPublishRequest::new(
            ClipboardKind::System,
            offer(DataSourceKind::Clipboard),
            None,
        )
        .unwrap();
        let debug = format!("{request:?}");
        assert!(debug.contains("format_count: 1"));
        assert!(!debug.contains("secret clipboard contents"));
        assert_eq!(
            NonZeroU64::new(1).map(ClipboardRevision::new),
            Some(ClipboardRevision::INITIAL)
        );
    }
}

use std::any::Any;
use std::cell::Cell;
use std::num::{NonZeroU16, NonZeroU32};
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, DataFormat, ExecutionRequirement, ExternalUri,
    FileDialogAdmission, FileDialogAdmissionError, FileDialogCapability, FileDialogCapabilityQuery,
    FileDialogFilter, FileDialogFilterError, FileDialogFilterRule, FileDialogLimitError,
    FileDialogLimits, FileDialogMode, FileDialogOperations, FileDialogOptions,
    FileDialogOptionsError, FileDialogRequest, FileDialogResult, FileDialogSelection,
    FileDialogSelectionError, FileDialogService, FileDialogServiceKey, FileExtension,
    FileExtensionError, MAX_FILE_DIALOG_FILTERS, MAX_FILE_EXTENSION_BYTES, PermissionState,
    RequestId, RequestOutcome, SandboxAccessGrant, SandboxAccessPolicy, SelectedResource,
    SelectedResourceAccess, SelectedResourceKind, SelectedResourceName, ServiceLookup,
    ServiceRegistry, SuggestedFileName, SuggestedFileNameError, Support, UserGestureGrant,
    UserGestureRequirement, ViewId,
};

fn one() -> NonZeroU16 {
    NonZeroU16::new(1).unwrap()
}

fn open_options(
    filters: Vec<FileDialogFilter>,
    selection_limit: u16,
    sandbox_access: SandboxAccessPolicy,
) -> FileDialogOptions {
    FileDialogOptions::new(
        FileDialogMode::OpenFile,
        filters,
        None,
        NonZeroU16::new(selection_limit).unwrap(),
        sandbox_access,
    )
    .unwrap()
}

fn image_filter(label: &str) -> FileDialogFilter {
    FileDialogFilter::new(
        label,
        vec![
            FileDialogFilterRule::Extension(FileExtension::new("png").unwrap()),
            FileDialogFilterRule::Format(DataFormat::mime("image/png").unwrap()),
        ],
    )
    .unwrap()
}

#[test]
fn filters_options_names_and_capability_limits_are_bounded_and_typed() {
    let extension = FileExtension::new(".PNG").unwrap();
    assert_eq!(extension.as_str(), "png");
    assert_eq!(FileExtension::new(""), Err(FileExtensionError::Empty));
    assert_eq!(
        FileExtension::new("bad/name"),
        Err(FileExtensionError::InvalidCharacter)
    );
    assert_eq!(FileExtension::new("é"), Err(FileExtensionError::NonAscii));
    assert_eq!(
        FileExtension::new("x".repeat(MAX_FILE_EXTENSION_BYTES + 1)),
        Err(FileExtensionError::TooLong)
    );

    let sensitive_label = "Private customer images";
    let filter = image_filter(sensitive_label);
    assert_eq!(filter.rules().len(), 2);
    let debug = format!("{filter:?}");
    assert!(debug.contains("label_redacted"));
    assert!(!debug.contains(sensitive_label));
    assert_eq!(
        FileDialogFilter::new("empty", vec![]),
        Err(FileDialogFilterError::EmptyRules)
    );
    let rule = FileDialogFilterRule::Extension(extension);
    assert_eq!(
        FileDialogFilter::new("duplicate", vec![rule.clone(), rule]),
        Err(FileDialogFilterError::DuplicateRule)
    );

    let sensitive_name = "quarterly-private-plan.txt";
    let suggested = SuggestedFileName::new(sensitive_name).unwrap();
    assert_eq!(suggested.as_str(), sensitive_name);
    assert!(!format!("{suggested:?}").contains(sensitive_name));
    assert_eq!(
        SuggestedFileName::new("../escape.txt"),
        Err(SuggestedFileNameError::PathLike)
    );
    assert_eq!(
        FileDialogOptions::new(
            FileDialogMode::OpenFile,
            vec![],
            Some(suggested.clone()),
            one(),
            SandboxAccessPolicy::PlatformDefault,
        ),
        Err(FileDialogOptionsError::SuggestedNameRequiresSave)
    );
    assert_eq!(
        FileDialogOptions::new(
            FileDialogMode::SaveFile,
            vec![],
            Some(suggested),
            NonZeroU16::new(2).unwrap(),
            SandboxAccessPolicy::PlatformDefault,
        ),
        Err(FileDialogOptionsError::SaveRequiresSingleSelection)
    );
    assert_eq!(
        FileDialogOptions::new(
            FileDialogMode::OpenFile,
            vec![filter; MAX_FILE_DIALOG_FILTERS + 1],
            None,
            one(),
            SandboxAccessPolicy::PlatformDefault,
        ),
        Err(FileDialogOptionsError::TooManyFilters)
    );

    assert_eq!(
        FileDialogLimits::new(
            NonZeroU16::new((MAX_FILE_DIALOG_FILTERS + 1) as u16).unwrap(),
            one(),
            one(),
            NonZeroU32::new(64).unwrap(),
        ),
        Err(FileDialogLimitError::FilterLimitTooLarge)
    );
}

struct FixtureSandboxGrant {
    dropped: Rc<Cell<bool>>,
}

impl SandboxAccessGrant for FixtureSandboxGrant {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl Drop for FixtureSandboxGrant {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}

fn resource(
    locator: &str,
    kind: SelectedResourceKind,
    access: SelectedResourceAccess,
    grant: Option<Box<dyn SandboxAccessGrant>>,
) -> SelectedResource {
    SelectedResource::new(
        kind,
        ExternalUri::new(locator).unwrap(),
        Some(SelectedResourceName::new("private-client-name.png").unwrap()),
        access,
        grant,
    )
}

#[test]
fn selections_use_redacted_uri_locators_and_linear_optional_sandbox_grants() {
    let view = ViewId::from_raw(4, 2).unwrap();
    let options = open_options(vec![], 2, SandboxAccessPolicy::RequireGrant);

    assert!(matches!(
        FileDialogSelection::new(view, &options, vec![]),
        Err(FileDialogSelectionError::Empty)
    ));
    assert!(matches!(
        FileDialogSelection::new(
            view,
            &options,
            vec![resource(
                "content://provider/private-missing-grant",
                SelectedResourceKind::File,
                SelectedResourceAccess::Read,
                None,
            )],
        ),
        Err(FileDialogSelectionError::SandboxGrantMissing { index: 0 })
    ));
    assert!(matches!(
        FileDialogSelection::new(
            view,
            &open_options(vec![], 1, SandboxAccessPolicy::PlatformDefault),
            vec![resource(
                "content://provider/folder",
                SelectedResourceKind::Folder,
                SelectedResourceAccess::Read,
                None,
            )],
        ),
        Err(FileDialogSelectionError::KindMismatch { index: 0 })
    ));
    assert!(matches!(
        FileDialogSelection::new(
            view,
            &open_options(vec![], 1, SandboxAccessPolicy::PlatformDefault),
            vec![resource(
                "content://provider/write-only",
                SelectedResourceKind::File,
                SelectedResourceAccess::Write,
                None,
            )],
        ),
        Err(FileDialogSelectionError::AccessMismatch { index: 0 })
    ));

    let grant_dropped = Rc::new(Cell::new(false));
    let selection = FileDialogSelection::new(
        view,
        &options,
        vec![resource(
            "content://provider/private-client-name.png?access=secret",
            SelectedResourceKind::File,
            SelectedResourceAccess::Read,
            Some(Box::new(FixtureSandboxGrant {
                dropped: Rc::clone(&grant_dropped),
            })),
        )],
    )
    .unwrap();
    assert_eq!(selection.view(), view);
    assert_eq!(selection.mode(), FileDialogMode::OpenFile);
    assert!(selection.resources()[0].has_sandbox_grant());
    assert_eq!(
        selection.resources()[0].locator().scheme().as_str(),
        "content"
    );
    let debug = format!("{selection:?}");
    assert!(debug.contains("has_sandbox_grant"));
    assert!(!debug.contains("private-client-name"));
    assert!(!debug.contains("secret"));

    let result = FileDialogResult::Selected(selection);
    assert!(!result.is_dismissed());
    assert_eq!(result.view(), view);
    assert_eq!(result.mode(), FileDialogMode::OpenFile);
    drop(result);
    assert!(grant_dropped.get());

    let dismissed = AdmittedRequest::new(RequestId::from_raw(9).unwrap()).complete(
        RequestOutcome::Applied(FileDialogResult::dismissed(view, FileDialogMode::OpenFile)),
    );
    assert!(dismissed.outcome().applied().unwrap().is_dismissed());
    let cancelled = AdmittedRequest::<FileDialogResult>::new(RequestId::from_raw(10).unwrap())
        .complete(RequestOutcome::Cancelled);
    assert!(cancelled.outcome().is_cancelled());
}

struct FixtureGestureGrant {
    view: ViewId,
    nonce: u64,
}

impl UserGestureGrant for FixtureGestureGrant {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

struct WrongGestureGrant;

impl UserGestureGrant for WrongGestureGrant {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

struct FixtureFileDialogService {
    view: ViewId,
    capability: FileDialogCapability,
    next_request: Cell<u64>,
}

impl FileDialogService for FixtureFileDialogService {
    fn capability(&self, query: FileDialogCapabilityQuery) -> Support<FileDialogCapability> {
        if query.view() == self.view {
            Support::Available(self.capability)
        } else {
            Support::Unavailable(telorgon::platform::UnavailableReason::UnavailableInScope)
        }
    }

    fn show(&self, request: FileDialogRequest) -> FileDialogAdmission {
        if request.view() != self.view {
            return Err(FileDialogAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        if self.capability.permission().blocks_use() {
            return Err(FileDialogAdmissionError::PermissionDenied);
        }
        let operations = *self.capability.operations();
        let options = request.options();
        if !operations.supports_mode(options.mode()) {
            return Err(FileDialogAdmissionError::UnsupportedMode {
                mode: options.mode(),
            });
        }
        if options.allows_multiple() && !operations.supports_multiple_selection() {
            return Err(FileDialogAdmissionError::MultipleSelectionUnsupported);
        }
        if options.sandbox_access() == SandboxAccessPolicy::RequireGrant
            && !operations.supports_sandbox_grants()
        {
            return Err(FileDialogAdmissionError::SandboxGrantUnavailable);
        }
        let limits = *self.capability.limits();
        if options.filters().len() > limits.maximum_filters().get() as usize {
            return Err(FileDialogAdmissionError::FiltersExceedCapability);
        }
        if options
            .filters()
            .iter()
            .any(|filter| filter.rules().len() > limits.maximum_rules_per_filter().get() as usize)
        {
            return Err(FileDialogAdmissionError::FilterRulesExceedCapability);
        }
        if options.selection_limit() > limits.maximum_selections() {
            return Err(FileDialogAdmissionError::SelectionsExceedCapability);
        }
        if options.suggested_name().is_some_and(|name| {
            name.byte_len() > limits.maximum_suggested_name_bytes().get() as usize
        }) {
            return Err(FileDialogAdmissionError::SuggestedNameExceedsCapability);
        }
        if self.capability.user_gesture().is_required() && !request.has_user_gesture() {
            return Err(FileDialogAdmissionError::UserGestureRequired);
        }

        let (_, _, grant) = request.into_parts();
        if self.capability.user_gesture().is_required() {
            let Some(grant) = grant else {
                unreachable!("required gesture was checked before request consumption")
            };
            let Ok(grant) = grant.into_any().downcast::<FixtureGestureGrant>() else {
                return Err(FileDialogAdmissionError::InvalidUserGesture);
            };
            if grant.view != self.view || grant.nonce != 77 {
                return Err(FileDialogAdmissionError::InvalidUserGesture);
            }
        }

        let request = self.next_request.get() + 1;
        self.next_request.set(request);
        Ok(AdmittedRequest::new(RequestId::from_raw(request).unwrap()))
    }
}

#[test]
fn service_capability_gesture_admission_completion_and_registry_are_object_safe() {
    let view = ViewId::from_raw(7, 1).unwrap();
    let other_view = ViewId::from_raw(8, 1).unwrap();
    let capability = CapabilityDescriptor::new(
        FileDialogOperations::new(true, true, true, true, true),
        FileDialogLimits::new(
            NonZeroU16::new(2).unwrap(),
            NonZeroU16::new(2).unwrap(),
            NonZeroU16::new(2).unwrap(),
            NonZeroU32::new(64).unwrap(),
        )
        .unwrap(),
        PermissionState::NotRequired,
        ExecutionRequirement::HostEventLoop,
        UserGestureRequirement::RecentRequired,
    );
    let service: Rc<dyn FileDialogService> = Rc::new(FixtureFileDialogService {
        view,
        capability,
        next_request: Cell::new(40),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<FileDialogServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<FileDialogServiceKey>() else {
        panic!("registered file-dialog service must be available");
    };
    assert!(
        service
            .capability(FileDialogCapabilityQuery::new(view))
            .is_available()
    );
    assert!(
        service
            .capability(FileDialogCapabilityQuery::new(other_view))
            .is_unavailable()
    );

    let options = open_options(
        vec![image_filter("Sensitive filter label")],
        2,
        SandboxAccessPolicy::RequireGrant,
    );
    let no_gesture = FileDialogRequest::new(view, options.clone());
    let debug = format!("{no_gesture:?}");
    assert!(debug.contains("has_user_gesture"));
    assert!(!debug.contains("Sensitive filter label"));
    assert!(matches!(
        service.show(no_gesture),
        Err(FileDialogAdmissionError::UserGestureRequired)
    ));
    assert!(matches!(
        service.show(FileDialogRequest::with_user_gesture(
            view,
            options.clone(),
            Box::new(WrongGestureGrant),
        )),
        Err(FileDialogAdmissionError::InvalidUserGesture)
    ));
    assert!(matches!(
        service.show(FileDialogRequest::with_user_gesture(
            view,
            options.clone(),
            Box::new(FixtureGestureGrant {
                view: other_view,
                nonce: 77,
            }),
        )),
        Err(FileDialogAdmissionError::InvalidUserGesture)
    ));

    let admitted = service
        .show(FileDialogRequest::with_user_gesture(
            view,
            options,
            Box::new(FixtureGestureGrant { view, nonce: 77 }),
        ))
        .unwrap();
    assert_eq!(admitted.request_id().get(), 41);
    let completion = admitted.complete(RequestOutcome::Applied(FileDialogResult::dismissed(
        view,
        FileDialogMode::OpenFile,
    )));
    assert!(completion.outcome().applied().unwrap().is_dismissed());
}

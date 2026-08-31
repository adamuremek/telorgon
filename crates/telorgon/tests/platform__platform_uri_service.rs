use std::any::Any;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, ExternalUri, ExternalUriError,
    MAX_EXTERNAL_URI_BYTES, MAX_URI_SCHEME_BYTES, MAX_URI_SCHEMES, PermissionState, RequestId,
    RequestOutcome, ServiceLookup, ServiceRegistry, Support, UriAdmissionError, UriCapabilities,
    UriCapabilityError, UriLimitError, UriLimits, UriOpenAdmission, UriOpenApplied, UriOpenRequest,
    UriOperation, UriScheme, UriSchemeCapability, UriSchemeError, UriService, UriServiceKey,
    UserGestureGrant, UserGestureRequirement, ViewId,
};

fn scheme_capability(
    scheme: &str,
    maximum_uri_bytes: u32,
    permission: PermissionState,
    gesture: UserGestureRequirement,
) -> UriSchemeCapability {
    UriSchemeCapability::new(
        UriScheme::new(scheme).unwrap(),
        CapabilityDescriptor::new(
            UriOperation::OpenExternal,
            UriLimits::new(NonZeroU32::new(maximum_uri_bytes).unwrap()).unwrap(),
            permission,
            ExecutionRequirement::HostEventLoop,
            gesture,
        ),
    )
}

#[test]
fn schemes_and_external_uris_are_bounded_absolute_and_debug_redacted() {
    let scheme = UriScheme::new("HTTPS").unwrap();
    assert_eq!(scheme.as_str(), "https");
    assert_eq!(scheme.byte_len(), 5);
    assert_eq!(UriScheme::new(""), Err(UriSchemeError::Empty));
    assert_eq!(
        UriScheme::new("1https"),
        Err(UriSchemeError::InvalidFirstCharacter)
    );
    assert_eq!(
        UriScheme::new("bad_scheme"),
        Err(UriSchemeError::InvalidCharacter)
    );
    assert_eq!(
        UriScheme::new("s".repeat(MAX_URI_SCHEME_BYTES + 1)),
        Err(UriSchemeError::TooLong {
            byte_len: MAX_URI_SCHEME_BYTES + 1,
            maximum_bytes: MAX_URI_SCHEME_BYTES,
        })
    );

    let sensitive = "HTTPS://user:secret@example.com/private?q=token%20value#fragment";
    let uri = ExternalUri::new(sensitive).unwrap();
    assert_eq!(uri.as_str(), sensitive);
    assert_eq!(uri.scheme(), &scheme);
    assert_eq!(uri.byte_len(), sensitive.len());
    let debug = format!("{uri:?}");
    assert!(debug.contains("https"));
    assert!(debug.contains("redacted"));
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("private"));

    assert_eq!(ExternalUri::new(""), Err(ExternalUriError::Empty));
    assert_eq!(
        ExternalUri::new("relative/path"),
        Err(ExternalUriError::MissingScheme)
    );
    assert!(matches!(
        ExternalUri::new("https://example.com/a b"),
        Err(ExternalUriError::WhitespaceOrControl { .. })
    ));
    assert!(matches!(
        ExternalUri::new("https://example.com/é"),
        Err(ExternalUriError::NonAscii { .. })
    ));
    assert!(matches!(
        ExternalUri::new("https://example.com/%ZZ"),
        Err(ExternalUriError::InvalidPercentEncoding { .. })
    ));
    assert!(matches!(
        ExternalUri::new("https://example.com\\private"),
        Err(ExternalUriError::InvalidCharacter { .. })
    ));
    let oversized = format!("x:{}", "a".repeat(MAX_EXTERNAL_URI_BYTES));
    assert_eq!(
        ExternalUri::new(&oversized),
        Err(ExternalUriError::TooLong {
            byte_len: oversized.len(),
            maximum_bytes: MAX_EXTERNAL_URI_BYTES,
        })
    );
}

#[test]
fn supported_scheme_capabilities_preserve_independent_policy_and_hard_limits() {
    assert_eq!(
        UriLimits::new(NonZeroU32::new(MAX_EXTERNAL_URI_BYTES as u32 + 1).unwrap()),
        Err(UriLimitError::UriByteLimitTooLarge)
    );
    let https = scheme_capability(
        "https",
        512,
        PermissionState::NotRequired,
        UserGestureRequirement::RecentRequired,
    );
    let mailto = scheme_capability(
        "mailto",
        256,
        PermissionState::PromptRequired,
        UserGestureRequirement::NotRequired,
    );
    let capabilities = UriCapabilities::new(vec![https.clone(), mailto.clone()]).unwrap();
    assert_eq!(capabilities.len(), 2);
    assert!(capabilities.supports(&UriScheme::new("HTTPS").unwrap()));
    assert_eq!(
        capabilities
            .capability(&UriScheme::new("mailto").unwrap())
            .unwrap()
            .permission(),
        PermissionState::PromptRequired
    );
    assert!(
        capabilities
            .capability(&UriScheme::new("https").unwrap())
            .unwrap()
            .user_gesture()
            .is_required()
    );
    assert!(https.admits(&ExternalUri::new("https://example.com").unwrap()));
    assert!(
        !https
            .admits(&ExternalUri::new(format!("https://example.com/{}", "x".repeat(512))).unwrap())
    );

    assert_eq!(
        UriCapabilities::new(vec![https.clone(), https]),
        Err(UriCapabilityError::DuplicateScheme {
            scheme: UriScheme::new("https").unwrap(),
        })
    );
    let too_many = (0..=MAX_URI_SCHEMES)
        .map(|index| {
            scheme_capability(
                &format!("s{index}"),
                128,
                PermissionState::NotRequired,
                UserGestureRequirement::NotRequired,
            )
        })
        .collect();
    assert_eq!(
        UriCapabilities::new(too_many),
        Err(UriCapabilityError::TooManySchemes {
            supplied: MAX_URI_SCHEMES + 1,
            maximum: MAX_URI_SCHEMES,
        })
    );
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

struct FixtureUriService {
    view: ViewId,
    capabilities: UriCapabilities,
    next_request: Cell<u64>,
}

impl UriService for FixtureUriService {
    fn capabilities(&self) -> Support<UriCapabilities> {
        Support::Available(self.capabilities.clone())
    }

    fn open(&self, request: UriOpenRequest) -> UriOpenAdmission {
        let scheme = request.uri().scheme().clone();
        if request.view() != self.view {
            return Err(UriAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        let Some(capability) = self.capabilities.capability(&scheme) else {
            return Err(UriAdmissionError::SchemeUnsupported { scheme });
        };
        if capability.permission().blocks_use() {
            return Err(UriAdmissionError::PermissionDenied { scheme });
        }
        let maximum_bytes = capability.limits().maximum_uri_bytes();
        if request.uri().byte_len() > maximum_bytes.get() as usize {
            return Err(UriAdmissionError::UriExceedsCapability {
                scheme,
                byte_len: request.uri().byte_len(),
                maximum_bytes,
            });
        }
        if capability.user_gesture().is_required() && !request.has_user_gesture() {
            return Err(UriAdmissionError::UserGestureRequired { scheme });
        }

        let (_, _, grant) = request.into_parts();
        if capability.user_gesture().is_required() {
            let Some(grant) = grant else {
                unreachable!("required grant was checked before consuming request")
            };
            let Ok(grant) = grant.into_any().downcast::<FixtureGestureGrant>() else {
                return Err(UriAdmissionError::InvalidUserGesture { scheme });
            };
            if grant.view != self.view || grant.nonce != 77 {
                return Err(UriAdmissionError::InvalidUserGesture { scheme });
            }
        }

        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        Ok(AdmittedRequest::new(RequestId::from_raw(next).unwrap()))
    }
}

#[test]
fn gesture_grants_are_view_scoped_consumed_and_adapter_validated_before_admission() {
    let view = ViewId::from_raw(7, 2).unwrap();
    let other_view = ViewId::from_raw(8, 1).unwrap();
    let https = scheme_capability(
        "https",
        256,
        PermissionState::NotRequired,
        UserGestureRequirement::RecentRequired,
    );
    let denied = scheme_capability(
        "blocked",
        256,
        PermissionState::Denied,
        UserGestureRequirement::NotRequired,
    );
    let service: Rc<dyn UriService> = Rc::new(FixtureUriService {
        view,
        capabilities: UriCapabilities::new(vec![https, denied]).unwrap(),
        next_request: Cell::new(40),
    });
    let mut registry = ServiceRegistry::new();
    assert!(registry.register::<UriServiceKey>(service).is_registered());
    let ServiceLookup::Available(service) = registry.lookup::<UriServiceKey>() else {
        panic!("registered URI service must be available");
    };
    assert_eq!(service.capabilities().into_available().unwrap().len(), 2);

    let sensitive = ExternalUri::new("https://example.com/private?token=secret").unwrap();
    let no_grant = UriOpenRequest::new(view, sensitive.clone());
    let debug = format!("{no_grant:?}");
    assert!(debug.contains("has_user_gesture"));
    assert!(!debug.contains("private"));
    assert!(!debug.contains("secret"));
    assert_eq!(
        service.open(no_grant),
        Err(UriAdmissionError::UserGestureRequired {
            scheme: UriScheme::new("https").unwrap(),
        })
    );

    let mismatched = UriOpenRequest::with_user_gesture(
        view,
        sensitive.clone(),
        Box::new(FixtureGestureGrant {
            view: other_view,
            nonce: 77,
        }),
    );
    assert_eq!(
        service.open(mismatched),
        Err(UriAdmissionError::InvalidUserGesture {
            scheme: UriScheme::new("https").unwrap(),
        })
    );

    let wrong_type =
        UriOpenRequest::with_user_gesture(view, sensitive.clone(), Box::new(WrongGestureGrant));
    assert_eq!(
        service.open(wrong_type),
        Err(UriAdmissionError::InvalidUserGesture {
            scheme: UriScheme::new("https").unwrap(),
        })
    );

    let request = UriOpenRequest::with_user_gesture(
        view,
        sensitive,
        Box::new(FixtureGestureGrant { view, nonce: 77 }),
    );
    let applied = UriOpenApplied::from_request(&request);
    let completion = service
        .open(request)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(completion.request_id().get(), 41);
    let applied = completion.outcome().applied().unwrap();
    assert_eq!(applied.view(), view);
    assert_eq!(applied.scheme(), &UriScheme::new("https").unwrap());
    assert!(!format!("{applied:?}").contains("secret"));

    assert_eq!(
        service.open(UriOpenRequest::new(
            view,
            ExternalUri::new("blocked:opaque").unwrap(),
        )),
        Err(UriAdmissionError::PermissionDenied {
            scheme: UriScheme::new("blocked").unwrap(),
        })
    );
    assert_eq!(
        service.open(UriOpenRequest::new(
            view,
            ExternalUri::new("ftp://example.com/file").unwrap(),
        )),
        Err(UriAdmissionError::SchemeUnsupported {
            scheme: UriScheme::new("ftp").unwrap(),
        })
    );
}

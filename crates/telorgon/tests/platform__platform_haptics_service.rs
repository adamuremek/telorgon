use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, HAPTIC_INTENSITY_UNITS,
    HapticAdmission, HapticAdmissionError, HapticApplied, HapticCapability, HapticCapabilityError,
    HapticDeviceSupport, HapticDeviceSupportError, HapticEffect, HapticEffectSupport,
    HapticIntensity, HapticIntensityError, HapticLimitError, HapticLimits, HapticOperations,
    HapticRequest, HapticUserSettingState, HapticsService, HapticsServiceKey, PermissionState,
    RequestId, RequestOutcome, ServiceLookup, ServiceRegistry, Support, UserGestureGrant,
    UserGestureRequirement,
};

#[test]
fn normalized_intensity_is_finite_bounded_and_exactly_stored() {
    assert_eq!(HapticIntensity::SILENT.units(), 0);
    assert_eq!(HapticIntensity::FULL.units(), HAPTIC_INTENSITY_UNITS);
    assert_eq!(
        HapticIntensity::from_normalized(0.375).unwrap().units(),
        375
    );
    assert_eq!(
        HapticIntensity::from_units(625).unwrap().normalized(),
        0.625
    );
    assert_eq!(
        HapticIntensity::from_units(HAPTIC_INTENSITY_UNITS + 1),
        Err(HapticIntensityError::AboveNormalizedMaximum {
            units: HAPTIC_INTENSITY_UNITS + 1,
        })
    );
    assert_eq!(
        HapticIntensity::from_normalized(-0.01),
        Err(HapticIntensityError::OutsideNormalizedRange)
    );
    assert_eq!(
        HapticIntensity::from_normalized(1.01),
        Err(HapticIntensityError::OutsideNormalizedRange)
    );
    assert_eq!(
        HapticIntensity::from_normalized(f32::NAN),
        Err(HapticIntensityError::NotFinite)
    );
    assert_eq!(
        HapticLimits::new(HapticIntensity::SILENT),
        Err(HapticLimitError::SilentMaximum)
    );
}

#[test]
fn semantic_effect_and_device_support_are_explicit_and_vendor_neutral() {
    let effects = HapticEffectSupport::only(HapticEffect::Selection)
        .with(HapticEffect::Activation)
        .with(HapticEffect::Success);
    assert_eq!(effects.count(), 3);
    assert!(effects.supports(HapticEffect::Selection));
    assert!(!effects.supports(HapticEffect::HeavyImpact));
    assert_eq!(
        effects.without(HapticEffect::Activation).count(),
        2,
        "effect sets must remain exact"
    );
    assert_eq!(HapticEffectSupport::all().count(), 9);

    assert_eq!(
        HapticDeviceSupport::available(HapticEffectSupport::none(), true),
        Err(HapticDeviceSupportError::NoSemanticEffects)
    );
    let device = HapticDeviceSupport::available(effects, true).unwrap();
    assert!(device.is_available());
    assert!(device.supports(HapticEffect::Success));
    assert!(device.supports_intensity_control());
    assert_eq!(device.effects(), Some(effects));
    assert!(!HapticDeviceSupport::Unavailable.is_available());
}

fn descriptor(
    maximum_intensity: HapticIntensity,
) -> CapabilityDescriptor<HapticOperations, HapticLimits> {
    CapabilityDescriptor::new(
        HapticOperations::new(true),
        HapticLimits::new(maximum_intensity).unwrap(),
        PermissionState::Granted,
        ExecutionRequirement::HostEventLoop,
        UserGestureRequirement::NotRequired,
    )
}

#[test]
fn capability_request_and_applied_result_preserve_policy_and_semantic_intention() {
    let effects = HapticEffectSupport::only(HapticEffect::MediumImpact).with(HapticEffect::Warning);
    let capability = HapticCapability::new(
        descriptor(HapticIntensity::from_units(800).unwrap()),
        HapticDeviceSupport::available(effects, true).unwrap(),
        HapticUserSettingState::Enabled,
    )
    .unwrap();
    assert!(capability.operations().supports_trigger());
    assert_eq!(capability.limits().maximum_intensity().units(), 800);
    assert_eq!(capability.permission(), PermissionState::Granted);
    assert_eq!(capability.execution(), ExecutionRequirement::HostEventLoop);
    assert_eq!(capability.user_setting(), HapticUserSettingState::Enabled);
    assert!(capability.user_setting().allows_feedback());
    assert!(capability.device().supports(HapticEffect::Warning));

    assert_eq!(
        HapticCapability::new(
            descriptor(HapticIntensity::from_units(800).unwrap()),
            HapticDeviceSupport::available(effects, false).unwrap(),
            HapticUserSettingState::Enabled,
        ),
        Err(HapticCapabilityError::FixedIntensityRequiresFullLimit)
    );

    let request = HapticRequest::new(
        HapticEffect::MediumImpact,
        HapticIntensity::from_units(650).unwrap(),
    );
    assert_eq!(request.effect(), HapticEffect::MediumImpact);
    assert_eq!(request.intensity().units(), 650);
    assert!(!request.has_user_gesture());
    assert!(format!("{request:?}").contains("has_user_gesture"));
    let applied = HapticApplied::from_request(&request);
    assert_eq!(applied.effect(), request.effect());
    assert_eq!(applied.intensity(), request.intensity());
}

struct Gesture;

impl UserGestureGrant for Gesture {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

struct FixtureHapticsService {
    capability: HapticCapability,
    next_request: Cell<u64>,
}

impl HapticsService for FixtureHapticsService {
    fn capability(&self) -> Support<HapticCapability> {
        Support::Available(self.capability)
    }

    fn trigger(&self, request: HapticRequest) -> HapticAdmission {
        if !self.capability.operations().supports_trigger() {
            return Err(HapticAdmissionError::UnsupportedOperation);
        }
        let device = self.capability.device();
        if !device.is_available() {
            return Err(HapticAdmissionError::DeviceUnavailable);
        }
        if !device.supports(request.effect()) {
            return Err(HapticAdmissionError::EffectUnsupported {
                effect: request.effect(),
            });
        }
        match self.capability.user_setting() {
            HapticUserSettingState::Enabled => {}
            HapticUserSettingState::Disabled => {
                return Err(HapticAdmissionError::UserSettingDisabled);
            }
            HapticUserSettingState::Unknown => {
                return Err(HapticAdmissionError::UserSettingUnknown);
            }
        }
        if self.capability.permission().blocks_use() {
            return Err(HapticAdmissionError::PermissionDenied);
        }
        if self.capability.permission().requires_prompt() {
            return Err(HapticAdmissionError::AuthorizationRequired);
        }
        if self.capability.user_gesture().is_required() && !request.has_user_gesture() {
            return Err(HapticAdmissionError::UserGestureRequired);
        }
        if !device.supports_intensity_control() && request.intensity() != HapticIntensity::FULL {
            return Err(HapticAdmissionError::IntensityControlUnsupported);
        }
        let maximum = self.capability.limits().maximum_intensity();
        if request.intensity() > maximum {
            return Err(HapticAdmissionError::IntensityExceedsCapability {
                requested: request.intensity(),
                maximum,
            });
        }

        let request_id = self.next_request.get() + 1;
        self.next_request.set(request_id);
        let admitted = AdmittedRequest::new(RequestId::from_raw(request_id).unwrap());
        let _adapter_owned_gesture = request.into_parts().2;
        Ok(admitted)
    }
}

fn fixture_service(
    device: HapticDeviceSupport,
    user_setting: HapticUserSettingState,
    gesture: UserGestureRequirement,
) -> Rc<dyn HapticsService> {
    let common = CapabilityDescriptor::new(
        HapticOperations::new(true),
        HapticLimits::default(),
        PermissionState::Granted,
        ExecutionRequirement::PlatformMainThread,
        gesture,
    );
    Rc::new(FixtureHapticsService {
        capability: HapticCapability::new(common, device, user_setting).unwrap(),
        next_request: Cell::new(40),
    })
}

#[test]
fn service_admission_completion_and_registry_are_linear_and_object_safe() {
    let effects = HapticEffectSupport::only(HapticEffect::Selection).with(HapticEffect::Success);
    let device = HapticDeviceSupport::available(effects, true).unwrap();
    let handle = fixture_service(
        device,
        HapticUserSettingState::Enabled,
        UserGestureRequirement::RecentRequired,
    );
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<HapticsServiceKey>(handle)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<HapticsServiceKey>() else {
        panic!("registered haptics service must be available");
    };
    assert!(service.capability().is_available());

    let missing_gesture = HapticRequest::new(HapticEffect::Success, HapticIntensity::FULL);
    assert_eq!(
        service.trigger(missing_gesture),
        Err(HapticAdmissionError::UserGestureRequired)
    );
    let request = HapticRequest::with_user_gesture(
        HapticEffect::Success,
        HapticIntensity::from_units(750).unwrap(),
        Box::new(Gesture),
    );
    let applied = HapticApplied::from_request(&request);
    let completion = service
        .trigger(request)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(completion.request_id().get(), 41);
    assert_eq!(
        completion.outcome().applied().unwrap().effect(),
        HapticEffect::Success
    );
    assert_eq!(
        completion.outcome().applied().unwrap().intensity().units(),
        750
    );

    let unsupported = HapticRequest::with_user_gesture(
        HapticEffect::Error,
        HapticIntensity::FULL,
        Box::new(Gesture),
    );
    assert_eq!(
        service.trigger(unsupported),
        Err(HapticAdmissionError::EffectUnsupported {
            effect: HapticEffect::Error,
        })
    );

    let disabled = fixture_service(
        device,
        HapticUserSettingState::Disabled,
        UserGestureRequirement::NotRequired,
    );
    assert_eq!(
        disabled.trigger(HapticRequest::new(
            HapticEffect::Selection,
            HapticIntensity::FULL,
        )),
        Err(HapticAdmissionError::UserSettingDisabled)
    );
    let unavailable = fixture_service(
        HapticDeviceSupport::Unavailable,
        HapticUserSettingState::Enabled,
        UserGestureRequirement::NotRequired,
    );
    assert_eq!(
        unavailable.trigger(HapticRequest::new(
            HapticEffect::Selection,
            HapticIntensity::FULL,
        )),
        Err(HapticAdmissionError::DeviceUnavailable)
    );
}

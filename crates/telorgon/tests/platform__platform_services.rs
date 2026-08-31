use std::cell::Cell;
use std::rc::Rc;

use telorgon::platform::services::{
    ServiceKey, ServiceLookup, ServiceRegistry, ServiceReplacement, ServiceUnavailable,
};

enum PrimaryCounter {}

impl ServiceKey for PrimaryCounter {
    type Handle = Rc<Cell<u32>>;
}

enum SecondaryCounter {}

impl ServiceKey for SecondaryCounter {
    type Handle = Rc<Cell<u32>>;
}

#[test]
fn public_service_registry_path_is_typed_local_and_has_no_implicit_fallback() {
    let mut registry = ServiceRegistry::new();
    assert!(matches!(
        registry.lookup::<PrimaryCounter>(),
        ServiceLookup::Unavailable(ServiceUnavailable::NotRegistered)
    ));

    assert!(
        registry
            .register::<PrimaryCounter>(Rc::new(Cell::new(4)))
            .is_registered()
    );
    assert!(
        registry
            .register::<SecondaryCounter>(Rc::new(Cell::new(9)))
            .is_registered()
    );
    let debug = format!("{registry:?}");
    assert!(debug.contains("registered_service_count: 2"));
    assert!(!debug.contains("Cell"));

    let primary = registry
        .lookup::<PrimaryCounter>()
        .into_available()
        .unwrap();
    assert_eq!(primary.get(), 4);
    primary.set(6);
    assert_eq!(
        registry
            .lookup::<SecondaryCounter>()
            .into_available()
            .unwrap()
            .get(),
        9
    );

    let replaced = registry.replace::<PrimaryCounter>(Rc::new(Cell::new(12)));
    assert!(matches!(replaced, ServiceReplacement::Replaced { .. }));
    assert_eq!(
        registry
            .remove::<PrimaryCounter>()
            .into_removed()
            .unwrap()
            .get(),
        12
    );
}

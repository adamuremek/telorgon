use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet};

use super::EntityId;

pub trait SceneComponent: Any + Send + Sync + 'static {}

impl<T> SceneComponent for T where T: Any + Send + Sync + 'static {}

trait ComponentStore {
    fn remove_entity(&mut self, entity: EntityId);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct TypedStore<T: SceneComponent> {
    items: BTreeMap<EntityId, T>,
}

impl<T: SceneComponent> Default for TypedStore<T> {
    fn default() -> Self {
        Self {
            items: BTreeMap::new(),
        }
    }
}

impl<T: SceneComponent> ComponentStore for TypedStore<T> {
    fn remove_entity(&mut self, entity: EntityId) {
        self.items.remove(&entity);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct SceneWorld {
    next_entity: u64,
    entities: BTreeSet<EntityId>,
    stores: BTreeMap<TypeId, Box<dyn ComponentStore>>,
}

impl SceneWorld {
    pub fn spawn(&mut self) -> EntityId {
        let entity = EntityId(self.next_entity);
        self.next_entity += 1;
        self.entities.insert(entity);
        entity
    }

    pub fn despawn(&mut self, entity: EntityId) -> bool {
        let removed = self.entities.remove(&entity);
        if !removed {
            return false;
        }

        for store in self.stores.values_mut() {
            store.remove_entity(entity);
        }

        true
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.entities.contains(&entity)
    }

    pub fn entities(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.iter().copied()
    }

    pub fn insert<T: SceneComponent>(&mut self, entity: EntityId, component: T) -> Option<T> {
        if !self.contains(entity) {
            return None;
        }

        let store = self.store_mut::<T>();
        store.items.insert(entity, component)
    }

    pub fn get<T: SceneComponent>(&self, entity: EntityId) -> Option<&T> {
        self.store::<T>()?.items.get(&entity)
    }

    pub fn get_mut<T: SceneComponent>(&mut self, entity: EntityId) -> Option<&mut T> {
        self.store_mut::<T>().items.get_mut(&entity)
    }

    pub fn remove<T: SceneComponent>(&mut self, entity: EntityId) -> Option<T> {
        self.store_mut::<T>().items.remove(&entity)
    }

    pub fn has<T: SceneComponent>(&self, entity: EntityId) -> bool {
        self.get::<T>(entity).is_some()
    }

    pub fn entities_with<T: SceneComponent>(&self) -> Vec<EntityId> {
        self.store::<T>()
            .map(|store| store.items.keys().copied().collect())
            .unwrap_or_default()
    }

    pub fn entities_with_pair<A: SceneComponent, B: SceneComponent>(&self) -> Vec<EntityId> {
        let Some(a) = self.store::<A>() else {
            return Vec::new();
        };
        let Some(b) = self.store::<B>() else {
            return Vec::new();
        };

        a.items
            .keys()
            .filter(|entity| b.items.contains_key(entity))
            .copied()
            .collect()
    }

    pub fn component_count<T: SceneComponent>(&self) -> usize {
        self.store::<T>()
            .map(|store| store.items.len())
            .unwrap_or(0)
    }

    fn store<T: SceneComponent>(&self) -> Option<&TypedStore<T>> {
        self.stores
            .get(&TypeId::of::<T>())
            .and_then(|store| store.as_any().downcast_ref::<TypedStore<T>>())
    }

    fn store_mut<T: SceneComponent>(&mut self) -> &mut TypedStore<T> {
        let store = self
            .stores
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(TypedStore::<T>::default()));

        store
            .as_any_mut()
            .downcast_mut::<TypedStore<T>>()
            .expect("scene component store type must match requested type")
    }
}

#[cfg(test)]
mod tests {
    use super::SceneWorld;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Marker(u32);

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Active(bool);

    #[test]
    fn entities_can_store_and_query_components() {
        let mut world = SceneWorld::default();
        let entity = world.spawn();

        world.insert(entity, Marker(7));
        world.insert(entity, Active(true));

        assert_eq!(world.component_count::<Marker>(), 1);
        assert_eq!(world.entities_with_pair::<Marker, Active>(), vec![entity]);
        assert_eq!(world.get::<Marker>(entity).unwrap().0, 7);
    }
}

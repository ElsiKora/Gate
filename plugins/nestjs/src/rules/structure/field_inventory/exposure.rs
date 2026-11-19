//! The API planes a field reaches, derived from the source.
//!
//! A DTO class reaches the plane of its file, of every generated route whose `dto` map and every
//! custom route whose `dto` map or `response` names it, and of every DTO class that nests it, to a
//! fixpoint; the planes of a nesting class flow to every class it nests, whatever members its
//! subclasses keep. A DTO field reaches the planes of its class and of every class that extends
//! it and keeps the field.
//!
//! A class extends another through its heritage clause, also through mapped types nested in each
//! other (`PickType`, `OmitType`, `PartialType`, `IntersectionType`), and an edge keeps a member
//! when, on some path through its mapped types, every `PickType` lists the member and no `OmitType`
//! does; a mapped type whose key list cannot be read filters nothing on its path.
//!
//! An entity field belongs to its declaring class and every class that extends it and keeps the
//! field. It reaches the planes of every DTO among those classes and of every DTO that copies it
//! (and of their subclasses that keep the copying property), the plane of every generated
//! controller over one of those entities whose generated DTOs carry it, and the plane of every
//! generated controller whose `GET_LIST` filters through a relation to one of them, as Automator
//! builds the filter baseline: `relation.member` for each column of the related entity, through a
//! to-one relation, or in a legacy plan also through a collection, or whose responses project the
//! related member as the relation's `reference.key`. A controller over a table inheritance root (an
//! `@Entity` with `@TableInheritance`) also carries the fields of its `@ChildEntity` classes in its
//! generated `GET` and `GET_LIST` responses, and the members of their to-one relations that those
//! responses project, as the variants of Crud Automator 5.4.0 hold them; none of its other
//! generated DTOs does, and none of its filters reaches a child's relation. A property counts as
//! Automator marks it: not with `isAutoDtoEnabled: false`, not where `properties` disables that
//! route and DTO type, not an `OBJECT` in a query, in `PARAMETERS` only as the primary key, in
//! `BODY` only when it is no primary key, and in a `GET_LIST` query unless the filter configuration
//! disables its path or rejects unlisted paths it does not list; a primary key is a filter only in
//! a typed filter plan, that is, under a filter configuration. A guard-restricted property counts
//! as exposed, so the derivation may over-approximate; the entry then pins that set.

use std::collections::{BTreeMap, BTreeSet};

use gate_typescript::owners::Owner;

use super::inventory::{Controller, Inventory};
use super::model::{AutoDto, Field, Key, Origin, Plane, Slot};

const QUERY: Slot = ("GET_LIST", "QUERY");

/// The generated responses that Crud Automator 5.4.0 builds, for a controller over a table
/// inheritance root, as one variant per `@ChildEntity` class with that class's own and inherited
/// columns. No other generated DTO of the root carries a child's column.
const VARIANTS: [Slot; 2] = [("GET", "RESPONSE"), ("GET_LIST", "RESPONSE")];

/// The classes that extend each class, transitively, and the planes they reach.
pub(super) struct Hierarchy<'i> {
    inventory: &'i Inventory,
    children: BTreeMap<&'i Owner, Vec<&'i Owner>>,
    /// The planes each class reaches itself: its path plane, the planes of the routes that name
    /// it, and the planes of the classes that nest it, propagated to a fixpoint.
    reach: BTreeMap<Owner, BTreeSet<Plane>>,
}

impl<'i> Hierarchy<'i> {
    pub(super) fn new(inventory: &'i Inventory) -> Self {
        let mut children: BTreeMap<&Owner, Vec<&Owner>> = BTreeMap::new();

        for (owner, info) in &inventory.classes {
            for base in &info.bases {
                children.entry(base).or_default().push(owner);
            }
        }

        let mut reach: BTreeMap<Owner, BTreeSet<Plane>> = inventory
            .classes
            .iter()
            .map(|(owner, info)| (owner.clone(), info.plane.into_iter().collect()))
            .collect();

        for (class, plane) in &inventory.routed {
            reach.entry(class.clone()).or_default().insert(*plane);
        }

        let mut hierarchy = Self {
            inventory,
            children,
            reach,
        };

        // A nested DTO reaches every plane its parent reaches; each pass only adds planes, so the
        // loop ends once no pass adds one.
        loop {
            let mut changed = false;

            for (parent, nested) in &inventory.nested {
                let planes = hierarchy.planes(parent);
                let entry = hierarchy.reach.entry(nested.clone()).or_default();
                let before = entry.len();
                entry.extend(planes);
                changed |= entry.len() != before;
            }

            if !changed {
                break;
            }
        }

        hierarchy
    }

    /// Whether the controller's own entity carries `@TableInheritance` and one of `holders`, the
    /// classes that carry a member, is a `@ChildEntity` class that extends it, so the variant of
    /// that class in the controller's generated read responses carries the member.
    fn variant(&self, controller: &Controller, holders: &BTreeSet<Owner>) -> bool {
        self.inventory
            .classes
            .get(&controller.entity)
            .is_some_and(|info| info.root)
            && self.descendants(&controller.entity).iter().any(|class| {
                holders.contains(class)
                    && self
                        .inventory
                        .classes
                        .get(class)
                        .is_some_and(|info| info.child)
            })
    }

    /// The class and every class that extends it.
    fn descendants(&self, owner: &Owner) -> BTreeSet<Owner> {
        self.descend(owner, None)
    }

    /// The class and every class that extends it through heritage edges that keep `member`: on
    /// some path of each edge, every `PickType` lists it and no `OmitType` does.
    pub(super) fn inheritors(&self, owner: &Owner, member: &str) -> BTreeSet<Owner> {
        self.descend(owner, Some(member))
    }

    fn descend(&self, owner: &Owner, member: Option<&str>) -> BTreeSet<Owner> {
        let mut found = BTreeSet::new();
        let mut pending = vec![owner.clone()];

        while let Some(current) = pending.pop() {
            if found.insert(current.clone()) {
                pending.extend(
                    self.children
                        .get(&current)
                        .into_iter()
                        .flatten()
                        .filter(|child| {
                            member
                                .is_none_or(|member| self.inventory.keeps(child, &current, member))
                        })
                        .map(|child| (*child).clone()),
                );
            }
        }

        found
    }

    /// The planes the class and every class that extends it reach.
    fn planes(&self, owner: &Owner) -> BTreeSet<Plane> {
        self.reached(&self.descendants(owner))
    }

    /// The planes the classes carrying `member` of `owner` reach.
    fn member_planes(&self, owner: &Owner, member: &str) -> BTreeSet<Plane> {
        self.reached(&self.inheritors(owner, member))
    }

    fn reached(&self, classes: &BTreeSet<Owner>) -> BTreeSet<Plane> {
        classes
            .iter()
            .filter_map(|class| self.reach.get(class))
            .flatten()
            .copied()
            .collect()
    }
}

/// The planes every field reaches.
pub(crate) fn derive(inventory: &Inventory) -> BTreeMap<Key, BTreeSet<Plane>> {
    let hierarchy = Hierarchy::new(inventory);

    inventory
        .fields
        .values()
        .map(|field| {
            let owner = Owner {
                name: field.key.class_name.clone(),
                path: Some(field.key.path.clone()),
            };
            let reached = match field.origin {
                Origin::Dto => hierarchy.member_planes(&owner, &field.key.member),
                Origin::Entity => entity_planes(inventory, &hierarchy, field, &owner),
            };

            (field.key.clone(), reached)
        })
        .collect()
}

/// The planes an entity field reaches through copies, generated DTOs and relation filters.
fn entity_planes(
    inventory: &Inventory,
    hierarchy: &Hierarchy<'_>,
    field: &Field,
    owner: &Owner,
) -> BTreeSet<Plane> {
    let member = field.key.member.as_str();
    let owners = hierarchy.inheritors(owner, member);
    let primary = inventory.primary.contains(&field.key);
    // A DTO that extends the entity, directly or through a mapped type that keeps the member,
    // carries the field.
    let mut reached = hierarchy.member_planes(owner, member);

    for copy in &inventory.copies {
        if owners.contains(&copy.target) && copy.member == member {
            reached.extend(hierarchy.member_planes(&copy.class, &copy.property));
        }
    }

    for controller in &inventory.controllers {
        let carried = |slot: &Slot| marked(&field.auto, primary, controller, *slot, member);
        let generated = owners.contains(&controller.entity) && controller.slots.iter().any(carried);
        let variant = hierarchy.variant(controller, &owners)
            && VARIANTS
                .iter()
                .any(|slot| controller.slots.contains(slot) && carried(slot));

        if generated || variant {
            reached.extend(controller.plane);
        }
    }

    for relation in &inventory.relations {
        if !owners.contains(&relation.target) {
            continue;
        }

        let holders = hierarchy.inheritors(&relation.entity, &relation.member);
        let path = format!("{}.{member}", relation.member);

        for controller in &inventory.controllers {
            let direct = holders.contains(&controller.entity);

            if !direct && !hierarchy.variant(controller, &holders) {
                continue;
            }

            let filtered = direct
                && controller.slots.contains(&QUERY)
                && (relation.to_one || !controller.typed_filter)
                && filterable(&relation.auto, QUERY)
                && filterable(&field.auto, QUERY)
                && listed(controller, &path);
            // A relation that a child entity holds reaches the root's controller only in its
            // response variants, each through the reference key of its own generated read route;
            // the root's `GET_LIST` query filters the root's own columns and relations.
            let referenced = relation.to_one
                && if direct {
                    controller.reference_keys.values().any(|key| key == member)
                } else {
                    VARIANTS.iter().any(|slot| {
                        controller.slots.contains(slot)
                            && controller
                                .reference_keys
                                .get(slot.0)
                                .is_some_and(|key| key == member)
                    })
                };

            if filtered || referenced {
                reached.extend(controller.plane);
            }
        }
    }

    reached
}

/// Whether a generated DTO of `slot` carries the entity field whose filter path is `path`.
pub(super) fn marked(
    auto: &AutoDto,
    primary: bool,
    controller: &Controller,
    slot: Slot,
    path: &str,
) -> bool {
    if !filterable(auto, slot) {
        return false;
    }

    match slot.1 {
        "PARAMETERS" => primary,
        "BODY" => !primary,
        "QUERY" => (controller.typed_filter || !primary) && listed(controller, path),
        _ => true,
    }
}

/// Whether Automator's property marking keeps the property for `slot` at all.
fn filterable(auto: &AutoDto, slot: Slot) -> bool {
    auto.described
        && !auto.disabled
        && !auto.excluded.contains(&slot)
        && (slot.1 != "QUERY" || !auto.object)
}

/// Whether the controller's `GET_LIST` filter configuration keeps the filter path.
fn listed(controller: &Controller, path: &str) -> bool {
    !controller.disabled_filters.contains(path)
        && controller
            .listed_filters
            .as_ref()
            .is_none_or(|listed| listed.contains(path))
}

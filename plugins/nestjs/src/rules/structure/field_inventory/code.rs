//! The code aspects of `nestjs/field-inventory`.
//!
//! Every `CODE` entry declares a code that matches its kind's grammar, so the field's Automator
//! `pattern` must be that grammar exactly, `/^[a-z0-9]+(-[a-z0-9]+)*$/` for a kebab-case code
//! (`code-format`). An entity code names its `role`: an `IDENTITY` code identifies its own record,
//! so no generated `UPDATE` or `PARTIAL_UPDATE` body of a controller over its entity, and no such
//! body the controller names that extends the entity, declares a property of the code's name or
//! copies it, may carry it (`code-immutable`), and every entity that carries it has a unique
//! index, unique constraint or primary key, its own or one it inherits, over exactly its `scope`
//! and the code (`code-unique`). Subscribers, triggers and custom write paths that
//! may still change a code are reviewed.

use std::collections::BTreeSet;

use gate_typescript::owners::Owner;

use super::Finding;
use super::exposure::{Hierarchy, marked};
use super::inventory::Inventory;
use super::model::{Class, Entry, Field, Key, Origin, Pattern, Role, Scope, Slot};

/// The generated request bodies that change a record.
const UPDATES: [Slot; 2] = [("UPDATE", "BODY"), ("PARTIAL_UPDATE", "BODY")];

/// The code aspects of one run.
pub(super) struct Codes<'i> {
    inventory: &'i Inventory,
    hierarchy: Hierarchy<'i>,
}

impl<'i> Codes<'i> {
    pub(super) fn new(inventory: &'i Inventory) -> Self {
        Self {
            inventory,
            hierarchy: Hierarchy::new(inventory),
        }
    }

    /// The code findings of one entry, whose options line is `entry_line`, about the source field
    /// it classifies.
    pub(super) fn findings(
        &self,
        entry: &Entry,
        field: Option<&Field>,
        entry_line: Option<u32>,
        options_path: &str,
    ) -> Vec<Finding> {
        let (Class::Code, Some(kind), Some(field)) = (entry.class, entry.kind, field) else {
            return Vec::new();
        };

        // A code Automator's format already names, or no free text, is a stale entry.
        if field.scope != Scope::Text {
            return Vec::new();
        }

        let key = entry.key();
        let at_field = |aspect: &'static str, message: String| Finding {
            aspect,
            path: key.path.clone(),
            line: Some(field.line),
            message,
            target: Some(format!("{}:{key}", key.path)),
            key: Some(key.to_string()),
        };
        let mut findings = Vec::new();

        match (field.origin, entry.role) {
            (Origin::Entity, None) => findings.push(Finding {
                aspect: "classification",
                path: options_path.to_owned(),
                line: entry_line,
                message: format!("entry {key} is a CODE of an entity and needs a role"),
                target: Some(format!("{}:{key}", key.path)),
                key: Some(format!("{key}#role")),
            }),
            (Origin::Dto, Some(role)) => findings.push(Finding {
                aspect: "classification",
                path: options_path.to_owned(),
                line: entry_line,
                message: format!(
                    "entry {key} is a CODE of a DTO, which carries no role, but names {}",
                    role.label()
                ),
                target: Some(format!("{}:{key}", key.path)),
                key: Some(format!("{key}#role")),
            }),
            _ => {}
        }

        let grammar = kind.grammar();
        let format = match &field.pattern {
            Pattern::Absent => Some("declares no Automator pattern".to_owned()),
            Pattern::Unread => {
                Some("has an Automator pattern nestjs/field-inventory cannot read".to_owned())
            }
            Pattern::Text(text) => (text
                .strip_prefix('/')
                .and_then(|body| body.strip_suffix('/'))
                != Some(grammar))
            .then(|| format!("has the Automator pattern {text}")),
        };

        if let Some(problem) = format {
            findings.push(at_field(
                "code-format",
                format!(
                    "{key} is a code of kind {} that {problem}, not /{grammar}/",
                    kind.label()
                ),
            ));
        }

        if field.origin == Origin::Entity && entry.role == Some(Role::Identity) {
            let owner = Owner {
                name: key.class_name.clone(),
                path: Some(key.path.clone()),
            };
            let writes = self.writes(field, &owner);

            if !writes.is_empty() {
                findings.push(at_field(
                    "code-immutable",
                    format!(
                        "{key} is an IDENTITY code that {} can change",
                        writes.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                ));
            }

            if let Some(problem) = self.uniqueness(&key, entry, &owner) {
                findings.push(at_field("code-unique", problem));
            }
        }

        findings
    }

    /// The generated update bodies, and the update bodies of its controllers that extend its
    /// entity, declare a property of its name or copy it, that carry the field.
    fn writes(&self, field: &Field, owner: &Owner) -> BTreeSet<String> {
        let member = field.key.member.as_str();
        let owners = self.hierarchy.inheritors(owner, member);
        let primary = self.inventory.primary.contains(&field.key);
        let mut writes = BTreeSet::new();

        for controller in &self.inventory.controllers {
            if !owners.contains(&controller.entity) {
                continue;
            }

            for slot in UPDATES {
                if controller.slots.contains(&slot)
                    && marked(&field.auto, primary, controller, slot, member)
                {
                    writes.insert(format!(
                        "the generated {} body of {}",
                        slot.0, controller.name
                    ));
                }
            }

            for body in &controller.update_bodies {
                // A body that extends the entity, also through a mapped type that keeps the
                // member, reaches its field among the classes it extends; a copy of the field
                // reaches the body when the body's heritage keeps the copying property.
                let carries = self.carriers(body, member).iter().any(|class| {
                    self.inventory.fields.keys().any(|key| {
                        key.member == member
                            && key.class_name == class.name
                            && Some(&key.path) == class.path.as_ref()
                    })
                }) || self.inventory.copies.iter().any(|copy| {
                    copy.member == member
                        && owners.contains(&copy.target)
                        && self.carriers(body, &copy.property).contains(&copy.class)
                });

                if carries {
                    writes.insert(format!(
                        "the update body {} of {}",
                        body.name, controller.name
                    ));
                }
            }
        }

        writes
    }

    /// The class and every class it extends through heritage edges that keep `member`: every
    /// `PickType` on the edge lists it and no `OmitType` does.
    fn carriers(&self, owner: &Owner, member: &str) -> BTreeSet<Owner> {
        let mut carriers = BTreeSet::from([owner.clone()]);
        let mut pending = vec![owner.clone()];

        while let Some(current) = pending.pop() {
            for base in self
                .inventory
                .classes
                .get(&current)
                .map(|info| info.bases.as_slice())
                .unwrap_or_default()
            {
                if self.inventory.keeps(&current, base, member) && carriers.insert(base.clone()) {
                    pending.push(base.clone());
                }
            }
        }

        carriers
    }

    /// The class and every class it extends.
    fn lineage(&self, owner: &Owner) -> BTreeSet<Owner> {
        let mut lineage = BTreeSet::from([owner.clone()]);
        let mut pending = vec![owner.clone()];

        while let Some(current) = pending.pop() {
            for base in self
                .inventory
                .classes
                .get(&current)
                .map(|info| info.bases.as_slice())
                .unwrap_or_default()
            {
                if lineage.insert(base.clone()) {
                    pending.push(base.clone());
                }
            }
        }

        lineage
    }

    /// Why an entity that carries the code has no unique index, unique constraint or primary key,
    /// its own or one it inherits, over exactly the code and its scope; `None` when every one has.
    fn uniqueness(&self, key: &Key, entry: &Entry, owner: &Owner) -> Option<String> {
        let mut wanted: BTreeSet<String> = entry.scope.iter().flatten().cloned().collect();
        wanted.insert(key.member.clone());
        let mut uncovered = Vec::new();
        let mut unread = BTreeSet::new();

        for entity in self.hierarchy.inheritors(owner, &key.member) {
            if !self
                .inventory
                .classes
                .get(&entity)
                .is_some_and(|info| info.entity)
            {
                continue;
            }

            let lineage = self.lineage(&entity);
            let primary: BTreeSet<String> = lineage
                .iter()
                .filter_map(|class| self.inventory.primary_keys.get(class))
                .flatten()
                .cloned()
                .collect();
            let covered = primary == wanted
                || lineage.iter().any(|class| {
                    self.inventory
                        .uniques
                        .get(class)
                        .is_some_and(|sets| sets.contains(&wanted))
                });

            if !covered {
                uncovered.push(entity.name.clone());
                // An unread constraint may be the one only when its columns are unknown or match.
                unread.extend(
                    lineage
                        .iter()
                        .filter(|class| {
                            self.inventory
                                .unread_uniques
                                .get(*class)
                                .is_some_and(|sets| {
                                    sets.iter()
                                        .any(|set| set.as_ref().is_none_or(|set| *set == wanted))
                                })
                        })
                        .map(|class| class.name.clone()),
                );
            }
        }

        if uncovered.is_empty() {
            return None;
        }

        let columns = wanted.into_iter().collect::<Vec<_>>().join(", ");
        let unread = if unread.is_empty() {
            String::new()
        } else {
            format!(
                "; nestjs/field-inventory cannot read a unique index or constraint of {}, which may be one",
                unread.into_iter().collect::<Vec<_>>().join(", ")
            )
        };

        Some(format!(
            "{key} is an IDENTITY code with no unique index, unique constraint or primary key over exactly {columns} on {}{unread}",
            uncovered.join(", ")
        ))
    }
}

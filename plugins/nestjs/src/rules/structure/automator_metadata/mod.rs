//! `nestjs/automator-metadata`: every persisted entity property carries one explicit
//! `ApiPropertyDescribe`, every DTO property one automator property decorator and no duplicate
//! validator, every controller handler exactly one `ApiRouteCustom` owner with only classified
//! local decorators, and every controller class only classified decorators.

mod inventory;
mod policy;
mod source;

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::{Location, Rule, Run};
use gate_typescript::owners::{Sources, class_declaration, find_class, top_level};

use self::inventory::{Entry, Unclassified};
pub use self::policy::Options;
use self::policy::Policy;
use self::source::{Semantic, Source};
use crate::layout::PluginOptions;
use crate::shared::project;
use crate::shared::report::Findings;
use crate::shared::syntax::typescript_sources;
use crate::shared::tree;

/// The rule.
#[derive(Debug, Default)]
pub struct AutomatorMetadata;

impl Rule<crate::Nest> for AutomatorMetadata {
    type Options = Options;

    const NAME: &'static str = "automator-metadata";
    const TITLE: &'static str =
        "Entity, DTO and route metadata follow the CRUD automator's contracts";
    const ASPECTS: &'static [&'static str] = &[
        "controller-decorator",
        "dto-decorator",
        "entity-description",
        "entity-property-decorator",
        "inventory-identity",
        "policy-key",
        "route-contract",
        "route-owner",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let policy = Policy::new(run.options(), &layout);
            let typescript = shared.typescript()?;
            let sources = shared.contexts(&layout.source_root)?;
            let bases = entity_bases(&sources, &policy);
            let mut entities = Vec::new();
            let mut dtos = Vec::new();
            let mut routes = Vec::new();
            let mut unclassified: Vec<Unclassified> = Vec::new();
            let mut specialized: Vec<Unclassified> = Vec::new();
            let mut findings = Findings::default();

            let modules: BTreeSet<&str> = typescript_sources(
                // Every folder: the listing that feeds all three inventories skips none.
                tree::files(typescript.source(), &policy.modules_root, &tree::UNSKIPPED),
            )
            .into_iter()
            .collect();
            let paths: BTreeSet<&str> = modules
                .iter()
                .copied()
                .chain(bases.keys().map(String::as_str))
                .collect();
            let no_bases = BTreeSet::new();

            for path in paths {
                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };

                let source = Source::new(path, &file, &policy);
                let file_bases = bases.get(path).unwrap_or(&no_bases);
                inventory::entities(&source, &policy, file_bases, &mut entities);
                inventory::specialized_entity_decorators(&source, file_bases, &mut specialized);

                // A base outside the modules root is read for its entity properties only.
                if modules.contains(path) {
                    inventory::dtos(&source, &policy, &mut dtos);
                    inventory::routes(&source, &policy, &mut routes, &mut unclassified);
                }
            }

            for decorator in unclassified {
                findings.push_keyed(
                    "controller-decorator",
                    decorator.path,
                    Some(decorator.line),
                    decorator.key,
                    decorator.message,
                );
            }

            for decorator in specialized {
                findings.push_keyed(
                    "entity-property-decorator",
                    decorator.path,
                    Some(decorator.line),
                    decorator.key,
                    decorator.message,
                );
            }

            compare("dtoProperties", &dtos, "dto-decorator", &mut findings);
            compare(
                "entityProperties",
                &entities,
                "entity-description",
                &mut findings,
            );
            compare("routes", &routes, "route-contract", &mut findings);
            owners(&routes, &mut findings);
            findings.report(run, help);
            stale(run, shared, &policy, &entities, &dtos, &routes);
            Ok(())
        })
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "controller-decorator" => "use only the classified controller decorators",
        "entity-property-decorator" => "describe entity fields with ApiPropertyDescribe only",
        "inventory-identity" => "give each class and member a name of its own",
        "route-owner" => "give the handler exactly one ApiRouteCustom owner",
        _ => {
            "make the metadata match the expected contract, or record the difference in the options"
        }
    }
}

/// The source classes, by file, that an `@Entity` or `@ChildEntity` class of a domain entity file
/// extends, directly or through other bases, through `base-depth` bases beyond the direct one,
/// whether or not a base is itself decorated; the walk does not restart at a decorated base. A base
/// is recorded once however many entities extend it, so its properties are listed once, under its
/// own name.
fn entity_bases(sources: &Sources<'_>, policy: &Policy) -> BTreeMap<String, BTreeSet<String>> {
    let mut bases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for entity_file in sources.iter() {
        if !inventory::is_entity_file(policy, &entity_file.path) {
            continue;
        }

        let source = Source::new(&entity_file.path, &entity_file.file, policy);

        for entity in top_level(&entity_file.file).filter_map(class_declaration) {
            if !inventory::is_entity_decorated(&source, entity) {
                continue;
            }

            let (mut file, mut class) = (entity_file, entity);

            for _ in 0..=policy.base_depth {
                let Some(heritage) = &class.heritage else {
                    break;
                };
                let identity = file.identity(&heritage.expression, false);
                let Some(owner) = sources.canonical(identity.as_ref()) else {
                    break;
                };
                let Some(base_file) = owner.path.as_deref().and_then(|path| sources.get(path))
                else {
                    break;
                };
                let Some(base) = find_class(&base_file.file, &owner.name) else {
                    break;
                };

                bases
                    .entry(base_file.path.clone())
                    .or_default()
                    .insert(owner.name.clone());
                (file, class) = (base_file, base);
            }
        }
    }

    bases
}

/// Reports duplicate keys and entries whose contract differs from the expected one.
fn compare(inventory: &str, entries: &[Entry], aspect: &'static str, findings: &mut Findings) {
    let mut keys: BTreeMap<&str, (usize, &Entry)> = BTreeMap::new();

    for entry in entries {
        keys.entry(&entry.key).or_insert((0, entry)).0 += 1;
    }

    for (key, (count, first)) in keys {
        if count != 1 {
            findings.push_keyed(
                "inventory-identity",
                first.path.as_str(),
                Some(first.line),
                format!("{inventory}:{key}"),
                format!(
                    "automator metadata {inventory} inventory contains {count} entries for {key}"
                ),
            );
        }
    }

    for entry in entries {
        let Some(expected) = &entry.expected else {
            continue;
        };
        let (expected, actual) = (expected.json(), entry.actual.json());

        if actual != expected {
            findings.push_keyed(
                aspect,
                entry.path.as_str(),
                Some(entry.line),
                entry.key.as_str(),
                format!(
                    "automator metadata {inventory} {} expected {expected}; received {actual}",
                    entry.key
                ),
            );
        }
    }
}

/// Reports route entries without exactly one owner.
fn owners(routes: &[Entry], findings: &mut Findings) {
    for entry in routes {
        let Semantic::Obj(actual) = &entry.actual else {
            continue;
        };
        let Some(owners @ Semantic::Arr(items)) = actual.get("owners") else {
            continue;
        };

        if items.len() != 1 {
            findings.push_keyed(
                "route-owner",
                entry.path.as_str(),
                Some(entry.line),
                entry.key.as_str(),
                format!(
                    "automator metadata routes {} must have exactly one owner; received {}",
                    entry.key,
                    owners.json()
                ),
            );
        }
    }
}

/// Reports each option entry whose key matches no inventory entry, at the entry's line in the
/// options file, or at `gate.toml` when the options are written there. The option names in the
/// messages are those of the automator's own inventory.
fn stale(
    run: &mut Run<'_, Options, crate::Nest>,
    shared: &project::Sources<'_>,
    policy: &Policy,
    entities: &[Entry],
    dtos: &[Entry],
    routes: &[Entry],
) {
    let keys = |entries: &[Entry]| -> BTreeSet<String> {
        entries.iter().map(|entry| entry.key.clone()).collect()
    };
    let (entity_keys, dto_keys, route_keys) = (keys(entities), keys(dtos), keys(routes));
    let mut stale: Vec<(String, String, String)> = Vec::new();
    let mut missing = |name: &str, key: &str, inventory: &str, observed: &BTreeSet<String>| {
        if !observed.contains(key) {
            stale.push((
                key.to_owned(),
                format!("{name}:{key}"),
                format!(
                    "automator metadata policy {name} entry {key} has no matching {inventory} \
                     source entry"
                ),
            ));
        }
    };

    for key in policy.hidden.keys() {
        missing("hiddenProperties", key, "entityProperties", &entity_keys);
    }

    for key in &policy.buffer {
        missing("bufferProperties", key, "entityProperties", &entity_keys);
    }

    for key in policy.decorators.keys() {
        missing("customDecorators", key, "dtoProperties", &dto_keys);
    }

    for key in policy.documentation.keys() {
        missing("documentationExceptions", key, "routes", &route_keys);
    }

    for key in policy.parameters.keys() {
        missing("parameterDecorators", key, "routes", &route_keys);
    }

    if stale.is_empty() {
        return;
    }

    let file = run
        .options_file()
        .map_or_else(|| "gate.toml".to_owned(), str::to_owned);
    let text = shared.text(&file).unwrap_or_default();

    for (key, id, message) in stale {
        let quoted = format!("\"{key}\"");
        let location = text
            .lines()
            .position(|line| line.contains(&quoted))
            .map_or_else(
                || Location::file(&file),
                |index| Location::line(&file, index + 1),
            );
        run.report()
            .finding("policy-key", location, message)
            .help("remove the entry, or correct its key to an existing member")
            .key(id);
    }
}

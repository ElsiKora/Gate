//! `nestjs/notification-egress`: outbound egress has a registered owner, and notifications leave
//! only through consumer classes.
//!
//! The options name the egress points, the owners that may use them and the consumer port. An
//! egress use of a `.ts` file below the source root (not `.d.ts`, outside the skipped folders) is
//! a runtime load of an egress module or a module below it, a runtime import of a transport symbol
//! whose source owner lies below the transport root, or a value reference to one of the global
//! egress functions, bare or as a member of the unshadowed `globalThis`.
//!
//! A file belongs to the owner with the deepest root that contains it. Package families close the
//! egress list: a runtime load of a package of a family is an egress module or a listed platform
//! client. A malformed set of options, an unknown class or an unaccepted decision stops the run.

mod scan;

use crate::shared::paths::{self, Form};
use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::owners::{Owner, Sources};

use self::scan::Scan;
use crate::layout::{PluginOptions, folder_prefix};
use crate::shared::json_lines::element_lines;
use crate::shared::report::Findings;
use crate::shared::{decisions, project};

/// The options of `nestjs/notification-egress`. `owner-classes`, `egress` and `owners` are
/// required.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The closed set of owner classes.
    #[schemars(length(min = 1), extend("uniqueItems" = true))]
    pub owner_classes: Vec<String>,
    /// The class of the owners that send notifications; without it no notification is checked.
    #[serde(default)]
    pub notification_class: Option<String>,
    /// The consumer port a notification leaves through; required with `notification-class`.
    #[serde(default)]
    pub consumer: Option<Consumer>,
    /// What a class extending the consumer port is called in messages; required with
    /// `notification-class`.
    #[serde(default)]
    pub consumer_label: Option<String>,
    /// The egress points.
    pub egress: Egress,
    /// The owners of egress.
    pub owners: Vec<OwnerEntry>,
    /// File names that compose owners without sending: barrels and module files.
    #[serde(default = "composition_files")]
    pub composition_files: Vec<String>,
    /// The global functions that send.
    #[serde(default = "global_egress_functions")]
    pub global_egress_functions: Vec<String>,
    /// The decision index; without it every decision counts as accepted.
    #[serde(default)]
    pub decisions: Option<String>,
}

fn composition_files() -> Vec<String> {
    vec!["index.ts".to_owned(), "module.ts".to_owned()]
}

fn global_egress_functions() -> Vec<String> {
    vec!["fetch".to_owned()]
}

/// The consumer port.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Consumer {
    /// The class name.
    pub name: String,
    /// The file that declares it.
    pub path: String,
}

/// The egress points and the packages that are not egress.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Egress {
    /// The egress modules; a module below one counts as it.
    pub modules: Vec<String>,
    /// Package-name prefixes whose every member is an egress module or a platform client.
    #[serde(default)]
    pub families: Vec<String>,
    /// Packages of a family that serve the platform rather than send egress.
    #[serde(default)]
    pub platform: Vec<Platform>,
    /// Network-capable modules kept off the egress list, each with its reason.
    #[serde(default)]
    pub excluded: Vec<Excluded>,
    /// The shared outbound transport; without it nothing is a transport.
    #[serde(default)]
    pub transport: Option<Transport>,
}

/// A network-capable module kept off the egress list.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Excluded {
    /// The module.
    pub module: String,
    /// Why it sends nothing.
    pub reason: String,
}

/// A package of a family that serves the platform.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Platform {
    /// The module.
    pub module: String,
    /// The accepted decisions that admit it.
    pub decisions: Vec<String>,
}

/// The shared outbound transport.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Transport {
    /// Its root, ending with a slash.
    pub root: String,
    /// The egress modules its files may load.
    pub modules: Vec<String>,
    /// The symbols through which other files send.
    pub symbols: Vec<String>,
    /// The accepted decision that admits it.
    pub decision: String,
}

/// One owner of egress.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct OwnerEntry {
    /// Its root, ending with a slash.
    pub root: String,
    /// Its classes, from `owner-classes`.
    pub classes: Vec<String>,
    /// The accepted decision that admits it.
    pub decision: String,
    /// Files outside the owner that may load it.
    #[serde(default)]
    pub senders: Vec<Sender>,
}

/// A file outside an owner that may load it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Sender {
    /// The file.
    pub path: String,
    /// The accepted decision that admits it.
    pub decision: String,
}

/// The rule.
#[derive(Debug, Default)]
pub struct NotificationEgress;

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if let Some(decisions) = &self.decisions {
            paths::check("decisions", decisions, Form::File)?;
        }
        if let Some(consumer) = &self.consumer {
            paths::check("consumer.path", &consumer.path, Form::File)?;
        }
        if let Some(transport) = &self.egress.transport {
            paths::check("egress.transport.root", &transport.root, Form::Prefix)?;
        }
        for (index, owner) in self.owners.iter().enumerate() {
            paths::check(&format!("owners[{index}].root"), &owner.root, Form::Prefix)?;
            for (sender, entry) in owner.senders.iter().enumerate() {
                paths::check(
                    &format!("owners[{index}].senders[{sender}].path"),
                    &entry.path,
                    Form::File,
                )?;
            }
        }
        Ok(())
    }
}

impl Rule<crate::Nest> for NotificationEgress {
    type Options = Options;

    const NAME: &'static str = "notification-egress";
    const TITLE: &'static str =
        "Egress has a registered owner and notifications leave through consumer classes";
    const ASPECTS: &'static [&'static str] = &[
        "direct-notification",
        "stale-owner",
        "unclassified-egress",
        "unclassified-package",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
                let layout = PluginOptions::read(run)?.layout;
                run.options().check_paths()?;
                let options = run.options();
                let options_file = run.options_file().map(str::to_owned);
                let label = options_file
                    .clone()
                    .unwrap_or_else(|| "the options in gate.toml".to_owned());
                let decisions = match &options.decisions {
                Some(path) => Some(decisions::accepted(&shared.text(path).ok_or_else(
                    || {
                        gate_sdk::Error::config(format!(
                            "{label}: the decision index {path} is not tracked; correct `decisions` \
                             or add the index"
                        ))
                    },
                )?)),
                None => None,
            };
                validate(options, &layout.source_root, decisions.as_ref(), &label)?;

                let (located, text) = match &options_file {
                    Some(file) => (file.clone(), shared.text(file).unwrap_or_default()),
                    None => ("gate.toml".to_owned(), String::new()),
                };
                let owner_lines = element_lines(&text, "owners");
                let typescript = shared.typescript()?;
                let sources = Sources::new(typescript, &layout.source_root)?;
                let mut findings = Findings::default();
                let scan = Scan {
                    sources: &sources,
                    options,
                    consumer: options.consumer.as_ref().map(|consumer| Owner {
                        name: consumer.name.clone(),
                        path: Some(consumer.path.clone()),
                    }),
                };
                let uses = scan.uses();
                let platform_loaded = scan.unclassified_packages(&mut findings);
                scan.unclassified_egress(&uses, &mut findings);
                let loaded = scan.direct_notifications(&mut findings);
                scan.stale_owners(&uses, &loaded, (&located, &owner_lines), &mut findings);
                scan.stale_egress(&uses, &platform_loaded, (&located, &text), &mut findings);
                findings.report(run, help);
                Ok(())
        })
    }
}

fn help(aspect: &str) -> &'static str {
    match aspect {
        "direct-notification" => {
            "send the notification from a class that extends the consumer port, or list the file \
             as a sender of its owner"
        }
        "stale-owner" => "remove the entry, or correct it to what the source holds",
        "unclassified-egress" => {
            "move the file under its egress owner, or register its root with a class and an \
             accepted decision"
        }
        _ => "list the package as an egress module or as a platform client with its decisions",
    }
}

fn invalid(label: &str, message: &str) -> gate_sdk::Error {
    gate_sdk::Error::config(format!("{label}: {message}"))
}

/// Stops the run on a repeated owner class, a notification class outside the owner classes, a
/// notification class without its consumer, an empty or repeated family, a transport module that
/// is repeated or no egress module, an excluded module without a reason, repeated or also an egress
/// module or platform client, a platform client without decisions, repeated or also an egress
/// module, a root that is not a unique folder below the source root, an unknown or repeated class,
/// or an unaccepted decision.
fn validate(
    options: &Options,
    source_root: &str,
    decisions: Option<&BTreeSet<String>>,
    label: &str,
) -> gate_sdk::Result<()> {
    let classes: BTreeSet<&str> = options.owner_classes.iter().map(String::as_str).collect();

    if classes.is_empty() || classes.len() != options.owner_classes.len() {
        return Err(invalid(label, "`owner-classes` must list each class once"));
    }

    if let Some(class) = &options.notification_class {
        if !classes.contains(class.as_str()) {
            return Err(invalid(
                label,
                &format!("the notification class {class:?} is not one of `owner-classes`"),
            ));
        }

        if options.consumer.is_none() || options.consumer_label.is_none() {
            return Err(invalid(
                label,
                "`notification-class` needs `consumer` and `consumer-label`",
            ));
        }
    }

    let egress = &options.egress;
    let mut named: Vec<&String> = Vec::new();
    validate_egress(egress, label, &mut named)?;
    validate_owners(options, &classes, source_root, label, &mut named)?;

    if let Some(decision) = named
        .into_iter()
        .find(|decision| decisions.is_some_and(|accepted| !accepted.contains(*decision)))
    {
        return Err(invalid(
            label,
            &format!("{decision} is not an accepted decision"),
        ));
    }

    Ok(())
}

fn validate_egress<'o>(
    egress: &'o Egress,
    label: &str,
    named: &mut Vec<&'o String>,
) -> gate_sdk::Result<()> {
    let mut families = BTreeSet::new();

    for family in &egress.families {
        if family.is_empty() || !families.insert(family.as_str()) {
            return Err(invalid(
                label,
                &format!("the family {family:?} is empty or repeated"),
            ));
        }
    }

    if let Some(transport) = &egress.transport {
        named.push(&transport.decision);
        let mut transported = BTreeSet::new();

        for module in &transport.modules {
            if !egress.modules.contains(module) || !transported.insert(module.as_str()) {
                return Err(invalid(
                    label,
                    &format!(
                        "the transport module {module:?} must be an egress module, listed once"
                    ),
                ));
            }
        }
    }

    let mut excluded = BTreeSet::new();

    for entry in &egress.excluded {
        if entry.reason.trim().is_empty()
            || egress.modules.contains(&entry.module)
            || egress
                .platform
                .iter()
                .any(|platform| platform.module == entry.module)
            || !excluded.insert(entry.module.as_str())
        {
            return Err(invalid(
                label,
                &format!(
                    "the excluded module {:?} needs a reason and must be listed once and neither \
                     as an egress module nor as a platform client",
                    entry.module
                ),
            ));
        }
    }

    let mut platforms = BTreeSet::new();

    for platform in &egress.platform {
        if platform.decisions.is_empty()
            || egress.modules.contains(&platform.module)
            || !platforms.insert(platform.module.as_str())
        {
            return Err(invalid(
                label,
                &format!(
                    "the platform client {:?} needs decisions and must be listed once and not as \
                     an egress module",
                    platform.module
                ),
            ));
        }

        named.extend(&platform.decisions);
    }

    Ok(())
}

fn validate_owners<'o>(
    options: &'o Options,
    classes: &BTreeSet<&str>,
    source_root: &str,
    label: &str,
    named: &mut Vec<&'o String>,
) -> gate_sdk::Result<()> {
    let prefix = folder_prefix(source_root);
    let mut roots = BTreeSet::new();
    let transport = options
        .egress
        .transport
        .as_ref()
        .map(|transport| &transport.root);

    for root in transport
        .into_iter()
        .chain(options.owners.iter().map(|owner| &owner.root))
    {
        if !root.starts_with(&prefix) || !root.ends_with('/') || !roots.insert(root.as_str()) {
            return Err(invalid(
                label,
                &format!(
                    "the root {root:?} is not a unique {prefix} directory ending with a slash"
                ),
            ));
        }
    }

    for owner in &options.owners {
        let own: BTreeSet<&str> = owner.classes.iter().map(String::as_str).collect();

        if own.is_empty()
            || own.len() != owner.classes.len()
            || own.iter().any(|class| !classes.contains(class))
        {
            return Err(invalid(
                label,
                &format!(
                    "the owner {} names the classes {:?}; give a non-empty set of {}",
                    owner.root,
                    owner.classes,
                    options.owner_classes.join(", ")
                ),
            ));
        }

        named.push(&owner.decision);
        named.extend(owner.senders.iter().map(|sender| &sender.decision));
    }

    Ok(())
}

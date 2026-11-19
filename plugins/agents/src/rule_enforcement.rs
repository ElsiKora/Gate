//! `agents/rule-enforcement`: every rule declares an enforcement its class can hold, and every
//! rule norm in the configuration names a rule enforced by Gate.

use gate_agents_format::Metadata;
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Result, Rule, Run, Tree};
use globset::GlobBuilder;

use crate::load::{Agents, at, default_root, strict_text};

/// The rule.
#[derive(Debug, Default)]
pub struct RuleEnforcement;

/// The options of `agents/rule-enforcement`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directory of the agent instructions, relative to the project.
    pub root: String,
    /// The roots an `eslint` enforcement may name as its last word.
    #[schemars(length(min = 1))]
    pub eslint_roots: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: default_root(),
            eslint_roots: vec!["root".to_owned()],
        }
    }
}

impl Rule for RuleEnforcement {
    type Options = Options;

    const NAME: &'static str = "rule-enforcement";
    const TITLE: &'static str = "Every rule declares a valid enforcement for its class";

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let tree = run.tree();
        let options = run.options();
        let agents = Agents::load(&tree, &options.root)?;
        let norms = run.norms().to_vec();
        let mut findings = Vec::new();

        for file in &agents.rules {
            for rule in &file.rules {
                let Ok(metadata) = rule.parse_metadata() else {
                    continue;
                };
                if metadata.status == "removed" {
                    continue;
                }
                if let Some(message) = problem(&tree, options, &norms, &rule.id, &metadata)? {
                    let line = rule.metadata.as_ref().map(|(line, _)| *line);
                    findings.push((Some(at(&file.path, line)), rule.id.clone(), message));
                }
            }
        }

        let registry = agents.registry();
        for norm in norms.iter().filter(|norm| norm.starts_with("RULE-")) {
            let message = match registry.get(norm.as_str()) {
                None => format!("a configured rule maps to {norm}, which no rule file defines"),
                Some((_, rule)) => match rule.parse_metadata() {
                    Ok(metadata) if metadata.class != "gate" => format!(
                        "a configured rule maps to {norm}, whose enforcement class is {}",
                        metadata.class
                    ),
                    _ => continue,
                },
            };
            findings.push((None, norm.clone(), message));
        }

        for (location, target, message) in findings {
            run.report()
                .finding("", location, message)
                .key(target.as_str())
                .target(target)
                .help(
                    "declare the enforcement its class needs (see the rule page), or map the \
                     norm only on rules whose rule file says `Enforcement: gate`",
                );
        }
        Ok(())
    }
}

/// Why a rule's declared enforcement is invalid, if it is.
fn problem(
    tree: &Tree<'_>,
    options: &Options,
    norms: &[String],
    id: &str,
    metadata: &Metadata,
) -> Result<Option<String>> {
    let detail = metadata.detail.as_deref().unwrap_or("");
    let tokens: Vec<&str> = detail.split_whitespace().collect();

    Ok(match metadata.class.as_str() {
        "gate" if !detail.is_empty() => {
            Some("gate enforcement takes no detail; the check is mapped in gate.toml".to_owned())
        }
        "gate" if !norms.iter().any(|norm| norm == id) => Some(format!(
            "{id} declares gate enforcement, but no check in gate.toml reports it"
        )),
        "eslint"
            if tokens.len() != 3
                || !tokens
                    .get(2)
                    .is_some_and(|root| options.eslint_roots.iter().any(|known| known == root)) =>
        {
            Some(format!(
                "eslint enforcement is `eslint <rule-id> <files-glob> <{}>`",
                options.eslint_roots.join("|")
            ))
        }
        "eslint"
            if tokens
                .get(1)
                .is_some_and(|glob| GlobBuilder::new(glob).build().is_err()) =>
        {
            Some(format!(
                "`{}` is not a valid glob",
                tokens.get(1).unwrap_or(&"")
            ))
        }
        "typecheck" if tokens.len() != 2 => {
            Some("typecheck enforcement is `typecheck <compiler-option> <tsconfig>`".to_owned())
        }
        "typecheck" => {
            let (option, config) = (tokens.first().unwrap_or(&""), tokens.get(1).unwrap_or(&""));
            match strict_text(tree, config)? {
                None => Some(format!("{config} is not tracked")),
                Some(text) if !text.contains(&format!("\"{option}\"")) => {
                    Some(format!("{config} does not set {option}"))
                }
                Some(_) => None,
            }
        }
        "test" if tokens.len() != 1 => {
            Some("test enforcement is `test <path of the owning test>`".to_owned())
        }
        "test" if !tokens.first().is_some_and(|test| tree.contains(test)) => Some(format!(
            "the owning test {} is not tracked",
            tokens.first().unwrap_or(&"")
        )),
        "review" | "advisory"
            if detail
                .strip_prefix("-- ")
                .is_none_or(|reason| reason.trim().is_empty()) =>
        {
            Some(format!(
                "{} enforcement needs `-- <reason>`",
                metadata.class
            ))
        }
        _ => None,
    })
}

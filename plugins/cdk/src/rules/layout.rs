//! `cdk/layout`: the app has the standard's files and directories, and below `lib/` only the
//! standard's directories and the declared ones.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};

use crate::options::PluginOptions;
use crate::rules::{Found, report};
use crate::sources::Cdk;
use crate::standard::{ENTRY, LIB_DIRECTORIES, REQUIRED_FILES, RESERVED};

/// The rule.
#[derive(Debug, Default)]
pub struct Layout;

impl Rule<Cdk> for Layout {
    type Options = NoOptions;

    const NAME: &'static str = "layout";
    const TITLE: &'static str = "The app has the standard's files and directories, and below lib/ only those and the declared ones";
    const ASPECTS: &'static [&'static str] = &[
        "missing",
        "unknown",
        "extra-missing",
        "stack-directory",
        "entry",
    ];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        let options = PluginOptions::read(run)?;
        let paths: BTreeSet<String> = run.tree().paths().into_iter().collect();
        let mut found = Vec::new();

        missing(&paths, &mut found);
        lib_entries(&paths, &options, &mut found);
        stacks(&paths, &mut found);

        for path in paths.iter().filter(|path| path.starts_with("bin/")) {
            if path != ENTRY {
                found.push((
                    "entry",
                    path.clone(),
                    path.clone(),
                    format!("{path} is in bin/, which holds only {ENTRY}"),
                    "move the code below lib/ and import it from bin/application.ts",
                ));
            }
        }

        for (aspect, path, key, message, help) in found {
            report(
                run,
                Found {
                    aspect,
                    path: &path,
                    line: None,
                    key,
                    message,
                    help,
                },
            );
        }

        Ok(())
    }
}

/// aspect, path, key, message, help
type Entry = (&'static str, String, String, String, &'static str);

fn missing(paths: &BTreeSet<String>, found: &mut Vec<Entry>) {
    for file in REQUIRED_FILES {
        if !paths.contains(file) {
            found.push((
                "missing",
                file.to_owned(),
                file.to_owned(),
                format!("{file} is a required file of a CDK app"),
                "add the file as the standard describes it",
            ));
        }
    }

    for (directory, job, required) in LIB_DIRECTORIES {
        let prefix = format!("lib/{directory}/");
        if required && !paths.iter().any(|path| path.starts_with(&prefix)) {
            found.push((
                "missing",
                format!("lib/{directory}"),
                format!("lib/{directory}"),
                format!("lib/{directory} is a required directory of a CDK app: {job}"),
                "add the directory with its files",
            ));
        }
    }

    if !paths
        .iter()
        .any(|path| path.starts_with("deploy/") && path.ends_with(".config.example.json"))
    {
        found.push((
            "missing",
            "deploy".to_owned(),
            "deploy".to_owned(),
            "deploy/ has no <environment>.config.example.json, the example of the configuration"
                .to_owned(),
            "track an example configuration, such as deploy/staging.config.example.json",
        ));
    }
}

fn lib_entries(paths: &BTreeSet<String>, options: &PluginOptions, found: &mut Vec<Entry>) {
    let mut directories = BTreeSet::new();

    for path in paths {
        let Some(rest) = path.strip_prefix("lib/") else {
            continue;
        };
        if let Some((directory, _)) = rest.split_once('/') {
            directories.insert(directory.to_owned());
        } else {
            found.push((
                "unknown",
                path.clone(),
                path.clone(),
                format!("{path} is a file directly in lib/, which holds only directories"),
                "move the file into the directory of its job",
            ));
        }
    }

    for directory in &directories {
        let standard = LIB_DIRECTORIES
            .iter()
            .any(|(name, _, _)| *name == directory.as_str());
        if standard || options.declares(directory) {
            continue;
        }
        let path = format!("lib/{directory}");
        let message = RESERVED
            .iter()
            .find(|(reserved, _)| *reserved == directory.as_str())
            .map_or_else(
                || {
                    format!(
                        "{path} is not a directory of the standard and is not declared in extra"
                    )
                },
                |(_, goes)| format!("{path} is a reserved name; its content goes to {goes}"),
            );
        found.push((
            "unknown",
            path.clone(),
            path,
            message,
            "move the content to the standard's directory, or declare the directory in \
             [plugins.cdk] options extra with its job",
        ));
    }

    for extension in &options.extra {
        if !directories.contains(&extension.name) {
            let path = format!("lib/{}", extension.name);
            found.push((
                "extra-missing",
                "lib".to_owned(),
                extension.name.clone(),
                format!("{path} is declared in extra but holds no tracked file"),
                "add the directory, or remove it from extra",
            ));
        }
    }
}

fn stacks(paths: &BTreeSet<String>, found: &mut Vec<Entry>) {
    let mut topics = BTreeSet::new();

    for path in paths {
        let Some(rest) = path.strip_prefix("lib/stack/") else {
            continue;
        };
        if let Some((topic, _)) = rest.split_once('/') {
            topics.insert(topic.to_owned());
        } else {
            found.push((
                "stack-directory",
                path.clone(),
                path.clone(),
                format!("{path} is directly in lib/stack/; each stack has a directory of its own"),
                "move the stack to lib/stack/<topic>/<topic>.stack.ts",
            ));
        }
    }

    for topic in topics {
        for file in [
            format!("lib/stack/{topic}/{topic}.stack.ts"),
            format!("lib/stack/{topic}/index.ts"),
            format!("lib/stack/{topic}/interface/properties.interface.ts"),
            format!("lib/stack/{topic}/interface/index.ts"),
        ] {
            if !paths.contains(&file) {
                found.push((
                    "stack-directory",
                    format!("lib/stack/{topic}"),
                    file.clone(),
                    format!("lib/stack/{topic} has no {file}"),
                    "add the file: a stack directory holds the stack, its barrel and its properties interface",
                ));
            }
        }
    }
}

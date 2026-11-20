//! The standard every CDK app follows, as constants: the paths, the names, the roles, the tags and
//! the policy packs. A project extends only what [`crate::options::PluginOptions`] lets it extend.

/// The files every app tracks, relative to the project root.
pub const REQUIRED_FILES: [&str; 5] = [
    "cdk.json",
    "package.json",
    "tsconfig.json",
    "bin/application.ts",
    "guard/naming.guard",
];

/// The one entry file of the app.
pub const ENTRY: &str = "bin/application.ts";

/// The command `cdk.json`'s `app` runs.
pub const APP_COMMAND: &str = "npx ts-node --prefer-ts-exts bin/application.ts";

/// The directories below `lib/`, each with its job and whether every app has it.
pub const LIB_DIRECTORIES: [(&str, &str, bool); 8] = [
    (
        "config",
        "the configuration's type, reader and parser",
        true,
    ),
    (
        "naming",
        "physical names, construct names, stack names and tags",
        true,
    ),
    (
        "validation",
        "checks of the configuration and of the synthesized app",
        false,
    ),
    (
        "deployment",
        "the stage that puts one environment's stacks together",
        true,
    ),
    ("stack", "one stack per directory", true),
    ("construct", "the constructs, by topic", false),
    ("synthesizer", "the stack synthesizer", false),
    ("aspect", "the aspects", false),
];

/// The names refused below `lib/`, even as a declared extension, with where their content goes.
pub const RESERVED: [(&str, &str); 18] = [
    ("stacks", "lib/stack"),
    ("constructs", "lib/construct"),
    ("configs", "lib/config"),
    ("aspects", "lib/aspect"),
    (
        "configuration",
        "lib/config (the type and reader) and deploy/ (the values)",
    ),
    (
        "environments",
        "lib/config (the type and reader) and deploy/ (the values)",
    ),
    (
        "settings",
        "lib/config (the type and reader) and deploy/ (the values)",
    ),
    ("names", "lib/naming"),
    ("tags", "lib/naming"),
    ("tagging", "lib/naming"),
    ("stages", "lib/deployment"),
    ("deployments", "lib/deployment"),
    ("utils", "the topic that uses it, as a .utility.ts file"),
    ("util", "the topic that uses it, as a .utility.ts file"),
    ("helpers", "the topic that uses it, as a .utility.ts file"),
    ("common", "the topic that uses it, as a .utility.ts file"),
    ("shared", "the topic that uses it, as a .utility.ts file"),
    (
        "constants",
        "the topic that uses it, as a .constant.ts file",
    ),
];

/// The roles a file name ends with, before `.ts`.
pub const ROLES: [&str; 14] = [
    "stack",
    "construct",
    "deployment",
    "aspect",
    "synthesizer",
    "interface",
    "type",
    "enum",
    "constant",
    "function",
    "utility",
    "configuration",
    "validation",
    "error",
];

/// The roles whose file exports one class, named from the file.
pub const CLASS_ROLES: [&str; 6] = [
    "stack",
    "construct",
    "deployment",
    "aspect",
    "synthesizer",
    "error",
];

/// The roles that live only below one directory of `lib/`.
pub const ROLE_PLACES: [(&str, &str); 3] = [
    ("stack", "lib/stack/"),
    ("construct", "lib/construct/"),
    ("deployment", "lib/deployment/"),
];

/// The role directories: a file inside one carries that role.
pub const ROLE_DIRECTORIES: [&str; 6] = [
    "interface",
    "type",
    "enum",
    "constant",
    "function",
    "utility",
];

/// The context key that names the configuration file.
pub const CONTEXT_KEY: &str = "configuration";

/// The packages `package.json` pins exactly.
pub const PINNED_PACKAGES: [&str; 4] = ["aws-cdk-lib", "aws-cdk", "constructs", "cdk-nag"];

/// The scripts `package.json` has.
pub const SCRIPTS: [&str; 3] = ["synth", "test", "lint"];

/// The tag keys every app applies.
pub const TAGS: [&str; 5] = ["Project", "Environment", "ManagedBy", "Application", "Name"];

/// The cdk-nag packs a project may declare.
pub const NAG_PACKS: [&str; 6] = [
    "AwsSolutionsChecks",
    "HIPAASecurityChecks",
    "NIST80053R4Checks",
    "NIST80053R5Checks",
    "PCIDSS321Checks",
    "ServerlessChecks",
];

/// The properties whose value AWS uses as a resource's name.
pub const PHYSICAL_NAMES: [&str; 31] = [
    "alarmName",
    "bucketName",
    "clusterIdentifier",
    "clusterName",
    "dashboardName",
    "deliveryStreamName",
    "eventBusName",
    "exportName",
    "functionName",
    "instanceIdentifier",
    "keyAlias",
    "loadBalancerName",
    "logGroupName",
    "managedPolicyName",
    "parameterName",
    "policyName",
    "queueName",
    "repositoryName",
    "restApiName",
    "roleName",
    "ruleName",
    "secretName",
    "securityGroupName",
    "serviceName",
    "stateMachineName",
    "streamName",
    "tableName",
    "targetGroupName",
    "topicName",
    "userPoolName",
    "vpcName",
];

/// The folders whose function files have a unit test.
pub const TESTED_FOLDERS: [&str; 2] = ["lib/naming/", "lib/config/"];

/// The roles of the files [`TESTED_FOLDERS`] test.
pub const TESTED_ROLES: [&str; 2] = ["function", "utility"];

/// Whether `name` is kebab case: lowercase letters and digits in words joined by `-`.
#[must_use]
pub fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

/// Whether `name` is Pascal case: an uppercase letter, then letters and digits.
#[must_use]
pub fn is_pascal(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

/// `network-load-balancer` as `NetworkLoadBalancer`.
#[must_use]
pub fn pascal(kebab: &str) -> String {
    kebab
        .split('-')
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect()
}

/// `NetworkLoadBalancer` as `network-load-balancer`; digits stay with the word before them.
#[must_use]
pub fn kebab(pascal: &str) -> String {
    let mut text = String::new();
    for (index, character) in pascal.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                text.push('-');
            }
            text.push(character.to_ascii_lowercase());
        } else {
            text.push(character);
        }
    }
    text
}

/// Whether `token` has the form of an AWS region: `eu-west-1`, `us-gov-east-1`.
#[must_use]
pub fn is_region(token: &str) -> bool {
    const AREAS: [&str; 9] = ["af", "ap", "ca", "eu", "il", "me", "mx", "sa", "us"];
    let words: Vec<&str> = token.split('-').collect();
    let (area, middle, number) = match words.as_slice() {
        [area, direction, number] => (*area, vec![*direction], *number),
        [area, "gov", direction, number] => (*area, vec!["gov", *direction], *number),
        _ => return false,
    };
    AREAS.contains(&area)
        && middle
            .iter()
            .all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase()))
        && !number.is_empty()
        && number.len() <= 2
        && number.chars().all(|character| character.is_ascii_digit())
}

/// The runs of exactly twelve digits in `text` that are not all zeros: the account ids it holds.
#[must_use]
pub fn account_ids(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut run = String::new();
    for character in text.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_digit() {
            run.push(character);
        } else {
            if run.len() == 12 && run.chars().any(|digit| digit != '0') {
                found.push(run.clone());
            }
            run.clear();
        }
    }
    found
}

/// The words of `text` that have the form of a region, split at anything but letters, digits
/// and `-`.
#[must_use]
pub fn regions(text: &str) -> Vec<&str> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .filter(|token| is_region(token))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{account_ids, is_kebab, is_pascal, is_region, kebab, pascal, regions};

    #[test]
    fn names_convert_both_ways() {
        assert_eq!(pascal("aurora-postgres"), "AuroraPostgres");
        assert_eq!(kebab("AuroraPostgres"), "aurora-postgres");
        assert_eq!(kebab("Ec2Instance"), "ec2-instance");
        assert!(is_kebab("public-asset-2"));
        assert!(!is_kebab("Public-asset"));
        assert!(!is_kebab("public_asset"));
        assert!(!is_kebab("public--asset"));
        assert!(is_pascal("VpcEndpoints"));
        assert!(!is_pascal("vpcEndpoints"));
        assert!(!is_pascal("Vpc-Endpoints"));
    }

    #[test]
    fn regions_and_accounts_are_found_in_text() {
        assert!(is_region("eu-west-1"));
        assert!(is_region("us-gov-east-1"));
        assert!(!is_region("east-west-1"));
        assert!(!is_region("eu-west"));
        assert_eq!(regions("arn:aws:s3:eu-central-1:x"), vec!["eu-central-1"]);
        let id = "1".repeat(12);
        assert_eq!(
            account_ids(&format!("arn:aws:iam::{id}:role/x")),
            vec![id.clone()]
        );
        assert_eq!(account_ids(&"0".repeat(12)), Vec::<String>::new());
        assert_eq!(account_ids(&"1".repeat(13)), Vec::<String>::new());
    }
}

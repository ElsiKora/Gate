//! `cdk/file-names`, `cdk/declarations` and `cdk/barrels`.

use crate::{DEPLOYMENT, app, check, seen};

const PHYSICAL: &str = "lib/naming/create-physical-name.function.ts";
const CONFIG_BARREL: &str = "lib/config/index.ts";
const NAMING_BARREL: &str = "lib/naming/index.ts";

#[test]
fn file_names_reports_case_bare_roles_unknown_roles_and_misplaced_roles() {
    let files = app()
        .with("lib/naming/CreateName.function.ts", "export {};\n")
        .with("lib/naming/create_name.function.ts", "export {};\n")
        .with("lib/naming/function.ts", "export {};\n")
        .with("lib/naming/utility.function.ts", "export {};\n")
        .with("lib/naming/create-name.helper.ts", "export {};\n")
        .with("lib/naming/create-name.ts", "export {};\n")
        .with("lib/naming/network.stack.ts", "export {};\n")
        .with("lib/config/interface/read.function.ts", "export {};\n");
    assert_eq!(
        check("file-names", &files),
        [
            seen("bare-role", "lib/naming/function.ts", None, "bare-role"),
            seen(
                "bare-role",
                "lib/naming/utility.function.ts",
                None,
                "bare-role"
            ),
            seen("case", "lib/naming/CreateName.function.ts", None, "case"),
            seen("case", "lib/naming/create_name.function.ts", None, "case"),
            seen("role", "lib/naming/create-name.helper.ts", None, "role"),
            seen("role", "lib/naming/create-name.ts", None, "role"),
            seen(
                "role-place",
                "lib/config/interface/read.function.ts",
                None,
                "role-place"
            ),
            seen(
                "role-place",
                "lib/naming/network.stack.ts",
                None,
                "role-place"
            ),
        ]
    );
}

#[test]
fn declarations_counts_merged_overloads_and_function_constants_as_one_function() {
    let files = app()
        .with(
            PHYSICAL,
            "export function createPhysicalName(name: string): string;\nexport function createPhysicalName(name: number): string;\nexport function createPhysicalName(name: unknown): string {\n  return String(name);\n}\n",
        )
        .with(
            "lib/naming/create-stack-name.function.ts",
            "export const createStackName = (topic: string): string => topic;\n",
        );
    assert_eq!(check("declarations", &files), []);
}

#[test]
fn declarations_count_wants_one_entity() {
    let files = app().edit(
        PHYSICAL,
        "export function createPhysicalName",
        "export function createName(): string {\n  return \"\";\n}\n\nexport function createPhysicalName",
    );
    assert_eq!(
        check("declarations", &files),
        [seen("count", PHYSICAL, None, "count")]
    );
}

#[test]
fn declarations_kind_wants_the_kind_of_the_role() {
    let files = app().with(
        "lib/config/interface/configuration.interface.ts",
        "export type IConfiguration = { readonly environment: string };\n",
    );
    assert_eq!(
        check("declarations", &files),
        [seen(
            "kind",
            "lib/config/interface/configuration.interface.ts",
            Some(1),
            "IConfiguration"
        )]
    );
}

#[test]
fn declarations_name_wants_the_class_named_from_its_file() {
    let files = app().edit(
        DEPLOYMENT,
        "export class ApplicationDeployment extends Stage",
        "export class Deployment extends Stage",
    );
    assert_eq!(
        check("declarations", &files),
        [seen("name", DEPLOYMENT, Some(8), "Deployment")]
    );
}

#[test]
fn declarations_nested_reports_a_declaration_below_the_top_level() {
    let files = app().edit(
        PHYSICAL,
        "  return `sample-",
        "  interface IPart {\n    readonly value: string;\n  }\n  return `sample-",
    );
    assert_eq!(
        check("declarations", &files),
        [seen("nested", PHYSICAL, Some(2), "IPart")]
    );
}

#[test]
fn barrels_accept_named_and_suffixed_re_exports() {
    let files = app().with(
        CONFIG_BARREL,
        "export * from \"./interface/index.js\";\nexport { readConfiguration } from \"./read-configuration.function.ts\";\n",
    );
    assert_eq!(check("barrels", &files), []);
}

#[test]
fn barrels_missing_reports_a_folder_with_files_and_no_index() {
    let files = app().without(NAMING_BARREL);
    assert_eq!(
        check("barrels", &files),
        [seen("missing", NAMING_BARREL, None, "index.ts")]
    );
}

#[test]
fn barrels_reexport_only_outside_and_explicit_export() {
    let files = app().edit(
        CONFIG_BARREL,
        "export * from \"./read-configuration.function\";\n",
        "export * from \"../naming\";\nexport const version = 1;\n",
    );
    assert_eq!(
        check("barrels", &files),
        [
            seen(
                "explicit-export",
                CONFIG_BARREL,
                None,
                "./read-configuration.function"
            ),
            seen("outside", CONFIG_BARREL, Some(2), "../naming"),
            seen("reexport-only", CONFIG_BARREL, Some(3), "owned-code"),
        ]
    );
}

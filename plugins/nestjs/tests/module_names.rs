//! `nestjs/module-names` against synthetic module trees.

mod host;

use host::{Row, SKIP_DIST, check, error, paths_with, row};

const RULE: &str = "module-names";

const OPTIONS: &str = r#"{
    "semantic-owners": [
        { "owner-stem": "LibraryCatalog", "resource-path": "src/modules/domain/catalog" },
        { "owner-stem": "Author", "resource-path": "src/modules/public/author" }
    ],
    "stale-module-paths": ["src/modules/domain/book/loan/module.ts", "src/modules/domain/book/review"],
    "token-replacements": [["read-ease", "readease"]],
    "resource-class-prefixes": [{ "resource": "staff", "class-prefix": "Staff" }]
}"#;

fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort();
    rows
}

fn finding(aspect: &str, path: &str, message: &str) -> Row {
    row(aspect, path, None, &format!("{path} {message}"))
}

/// Module files that miss their names, semantic owner files and stale paths.
const MISNAMED: [(&str, &str); 17] = [
    (
        "src/modules/domain/book/module.ts",
        "export class WrongModule {}\n",
    ),
    (
        "src/modules/domain/shelf/module.ts",
        "export class ShelfDomainModule {}\n",
    ),
    (
        "src/modules/integration/read-ease/module.ts",
        "export class IntegrationReadEaseModule {}\n",
    ),
    (
        "src/modules/public/book/module.ts",
        "export class BookModule {}\n",
    ),
    (
        "src/modules/domain/staff/role/module.ts",
        "export class RoleModule {}\n",
    ),
    (
        "src/modules/domain/staff/module.ts",
        "export class TeamModule {}\n",
    ),
    (
        "src/modules/domain/plain/module.ts",
        "export class Plain {}\n",
    ),
    (
        "src/modules/domain/note/module.ts",
        "// export class NoteModule {}\nexport const note = 1;\nexport { NoteModule } from \"./other\";\n",
    ),
    (
        "src/modules/domain/anonymous/module.ts",
        "export default class {}\n",
    ),
    (
        "src/modules/domain/first/module.ts",
        "export default class {}\nexport class FirstModule {}\n",
    ),
    (
        "src/modules/domain/catalog/module.ts",
        "export class CatalogModule {}\n",
    ),
    (
        "src/modules/domain/catalog/entity/entity.ts",
        "export class CatalogEntity {}\n",
    ),
    (
        "src/modules/domain/catalog/service.ts",
        "export const catalog = 1;\n",
    ),
    (
        "src/modules/public/author/module.ts",
        "export class AuthorModule {}\n",
    ),
    (
        "src/modules/domain/book/loan/module.ts",
        "export class BookLoanModule {}\n",
    ),
    (
        "src/modules/domain/book/review/code/module.ts",
        "export class BookReviewCodeModule {}\n",
    ),
    ("src/modules/domain/broken/module.ts", "class {"),
];

#[test]
fn module_classes_and_semantic_owners_that_miss_their_names_are_reported() {
    assert_eq!(
        sorted(check(RULE, OPTIONS, &MISNAMED)),
        sorted(vec![
            finding(
                "resource-folder",
                "src/modules/domain/book/module.ts",
                "is under book; WrongModule should live under wrong"
            ),
            finding(
                "resource-folder",
                "src/modules/domain/shelf/module.ts",
                "is under shelf; ShelfDomainModule should live under shelf-domain"
            ),
            finding(
                "resource-folder",
                "src/modules/integration/read-ease/module.ts",
                "is under read-ease; IntegrationReadEaseModule should live under readease"
            ),
            finding(
                "module-owner-prefix",
                "src/modules/public/book/module.ts",
                "exports BookModule; modules in this owner must use the Public prefix"
            ),
            finding(
                "module-owner-prefix",
                "src/modules/domain/staff/role/module.ts",
                "exports RoleModule; modules in this owner must use the Staff prefix"
            ),
            finding(
                "module-owner-prefix",
                "src/modules/domain/staff/module.ts",
                "exports TeamModule; modules in this owner must use the Staff prefix"
            ),
            finding(
                "module-class",
                "src/modules/domain/plain/module.ts",
                "exports Plain; module classes must end with Module"
            ),
            finding(
                "module-class",
                "src/modules/domain/note/module.ts",
                "does not export a module class"
            ),
            finding(
                "module-class",
                "src/modules/domain/anonymous/module.ts",
                "does not export a module class"
            ),
            finding(
                "module-class",
                "src/modules/domain/first/module.ts",
                "does not export a module class"
            ),
            finding(
                "semantic-owner-class",
                "src/modules/domain/catalog/module.ts",
                "exports CatalogModule; semantic owner requires LibraryCatalogModule"
            ),
            finding(
                "semantic-owner-class",
                "src/modules/domain/catalog/entity/entity.ts",
                "exports CatalogEntity; semantic owner requires LibraryCatalogEntity"
            ),
            finding(
                "semantic-owner-class",
                "src/modules/domain/catalog/service.ts",
                "does not export LibraryCatalogService"
            ),
            finding(
                "module-owner-prefix",
                "src/modules/public/author/module.ts",
                "exports AuthorModule; modules in this owner must use the Public prefix"
            ),
            finding(
                "shortened-resource-folder",
                "src/modules/domain/book/loan/module.ts",
                "is a shortened resource folder; use the full entity resource name"
            ),
            finding(
                "shortened-resource-folder",
                "src/modules/domain/book/review",
                "is a shortened resource folder; use the full entity resource name"
            ),
        ])
    );
}

#[test]
fn module_classes_that_name_their_folder_pass() {
    let files = [
        (
            "src/modules/domain/book/module.ts",
            "import { Module } from \"@nestjs/common\";\n\n@Module({})\nexport class BookModule {}\n",
        ),
        (
            "src/modules/internal/book/loan/module.ts",
            "export class InternalBookLoanModule {}\n",
        ),
        (
            "src/modules/public/http-server/module.ts",
            "export class PublicHTTPServerModule {}\n",
        ),
        (
            "src/modules/integration/readease/module.ts",
            "export class IntegrationReadEaseModule {}\n",
        ),
        (
            "src/modules/integration/spread-ease/module.ts",
            "export class IntegrationSpreadEaseModule {}\n",
        ),
        (
            "src/infrastructure/s3/module.ts",
            "export class InfrastructureS3Module {}\n",
        ),
        (
            "src/modules/domain/staff/policy/module.ts",
            "export class StaffPolicyModule {}\n",
        ),
        (
            "src/modules/domain/staffing/module.ts",
            "export class StaffingModule {}\n",
        ),
        (
            "src/modules/domain/oauth2-client/module.ts",
            "export class Oauth2ClientModule {}\n",
        ),
        (
            "src/modules/domain/catalog/module.ts",
            "export class LibraryCatalogModule {}\n",
        ),
        (
            "src/modules/domain/catalog/entity/entity.ts",
            "export class LibraryCatalogEntity {}\n",
        ),
        (
            "src/modules/domain/catalog/service.ts",
            "export class LibraryCatalogService {}\n",
        ),
        (
            "src/modules/domain/declared/module.ts",
            "// export class WrongModule {}\nconst text = \"export class WrongModule {}\";\nexport declare class DeclaredModule {}\n",
        ),
        (
            "src/modules/domain/fallback/module.ts",
            "export default class FallbackModule {}\n",
        ),
        (
            "src/modules/domain/second/module.ts",
            "export class SecondModule {}\nexport class OtherModule {}\n",
        ),
        (
            "src/modules/domain/abstract/module.ts",
            "export abstract class AbstractModule {}\n",
        ),
        ("src/modules/other/module.ts", "export class Anything {}\n"),
        ("src/runtime/api/module.ts", "export class Anything {}\n"),
        (
            "src/modules/domain/book/submodule.ts",
            "export class Anything {}\n",
        ),
        (
            "src/modules/domain/book/review.md",
            "Not the stale module.\n",
        ),
    ];

    assert_eq!(check(RULE, OPTIONS, &files), []);
}

#[test]
fn without_options_no_resource_needs_an_extra_prefix_and_nothing_is_stale() {
    let files = [
        (
            "src/modules/domain/staff/role/module.ts",
            "export class StaffRoleModule {}\n",
        ),
        (
            "src/modules/domain/book/loan/module.ts",
            "export class BookLoanModule {}\n",
        ),
        (
            "src/modules/integration/read-ease/module.ts",
            "export class IntegrationReadEaseModule {}\n",
        ),
    ];

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn scopes_module_file_suffix_and_class_files_are_options() {
    let files = [
        (
            "src/features/book/feature.ts",
            "export class FeatureBookFeature {}\n",
        ),
        (
            "src/features/shelf/feature.ts",
            "export class ShelfFeature {}\n",
        ),
        (
            "src/features/note/feature.ts",
            "export class FeatureNote {}\n",
        ),
        (
            "src/features/catalog/feature.ts",
            "export class FeatureCatalogFeature {}\n",
        ),
        (
            "src/features/catalog/repository.ts",
            "export class CatalogRepository {}\n",
        ),
        (
            "src/features/catalog/service.ts",
            "export class Anything {}\n",
        ),
        (
            "src/modules/domain/book/module.ts",
            "export class WrongModule {}\n",
        ),
    ];
    let options = r#"{
        "layer-scopes": [{ "path": "src/features/", "class-prefix": "Feature" }],
        "module-file": "feature.ts",
        "module-suffix": "Feature",
        "semantic-class-files": [{ "file": "repository.ts", "suffix": "Repository" }],
        "semantic-owners": [{ "owner-stem": "FeatureCatalog", "resource-path": "src/features/catalog" }]
    }"#;

    assert_eq!(
        sorted(check(RULE, options, &files)),
        sorted(vec![
            finding(
                "module-owner-prefix",
                "src/features/shelf/feature.ts",
                "exports ShelfFeature; modules in this owner must use the Feature prefix"
            ),
            finding(
                "module-class",
                "src/features/note/feature.ts",
                "exports FeatureNote; module classes must end with Feature"
            ),
            finding(
                "semantic-owner-class",
                "src/features/catalog/repository.ts",
                "exports CatalogRepository; semantic owner requires FeatureCatalogRepository"
            ),
        ])
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "semanticOwnerStems": [] }"#, &[]).contains("semanticOwnerStems"));
    assert!(
        error(
            RULE,
            r#"{ "layer-scopes": [{ "path": "src/", "prefix": "" }] }"#,
            &[]
        )
        .contains("prefix")
    );
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let files = [
        (
            "src/modules/domain/dist/module.ts",
            "export class WrongModule {}\n",
        ),
        (
            "src/modules/domain/generated/module.ts",
            "export class WrongModule {}\n",
        ),
    ];

    assert_eq!(
        paths_with(RULE, "{}", SKIP_DIST, &files),
        [
            "src/modules/domain/dist/module.ts",
            "src/modules/domain/generated/module.ts"
        ]
    );
}

#[test]
fn the_project_root_is_refused_as_a_stale_module_path() {
    let message = error(
        RULE,
        r#"{ "stale-module-paths": ["src/modules", ""] }"#,
        &[],
    );

    assert!(
        message.contains("stale-module-paths[1]") && message.contains("project root"),
        "{message}"
    );
}

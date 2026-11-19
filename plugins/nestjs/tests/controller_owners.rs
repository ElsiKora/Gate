//! `nestjs/controller-owners` against a synthetic registry, controllers and services.

mod support;

use support::{Row, check, error, row, rows, run_from};

const RULE: &str = "controller-owners";
const ENTITY: &str = "src/modules/domain/book/entity.ts";
const SERVICE: &str = "src/modules/domain/book/service.ts";
const CONTROLLER: &str = "src/modules/internal/book/controller.ts";

const ENTITY_TEXT: &str = "export class BookEntity {}\nexport class ShelfEntity {}\n";
const SERVICE_TEXT: &str = "import { ApiService, ApiServiceBase } from \"@elsikora/nestjs-crud-automator\";\n\
                            import { BookEntity } from \"./entity\";\n\
                            @ApiService<BookEntity>({ entity: BookEntity })\n\
                            export class BookService extends ApiServiceBase<BookEntity> {}\n";
const CONTROLLER_TEXT: &str = "import { ApiController, IApiControllerBase } from \"@elsikora/nestjs-crud-automator\";\n\
                               import { Get } from \"@nestjs/common\";\n\
                               import { BookEntity } from \"../../domain/book/entity\";\n\
                               import { BookService } from \"../../domain/book/service\";\n\
                               @ApiController<BookEntity>({ entity: BookEntity, name: \"InternalBook\" })\n\
                               export class InternalBookController implements IApiControllerBase<BookEntity> {\n\
                               \x20 constructor(public service: BookService) {}\n\
                               \x20 @Get()\n\
                               \x20 public list(): void {}\n\
                               }\n";

fn registry(rows: &str) -> String {
    format!("{{ \"controllers\": [{rows}] }}")
}

const BOOK_ROW: &str = r#"{
    "path": "src/modules/internal/book/controller.ts",
    "controller-name": "InternalBookController",
    "classification": "ENTITY_RESOURCE",
    "primary-service-name": "BookService",
    "primary-service-path": "src/modules/domain/book/service.ts",
    "entity-name": "BookEntity",
    "entity-path": "src/modules/domain/book/entity.ts"
}"#;

fn finding(aspect: &str, path: &str, line: Option<u32>, message: &str) -> Row {
    row(aspect, path, line, message)
}

#[test]
fn a_registered_controller_with_exact_metadata_passes() {
    let files = [
        (ENTITY, ENTITY_TEXT),
        (SERVICE, SERVICE_TEXT),
        (CONTROLLER, CONTROLLER_TEXT),
    ];

    assert_eq!(check(RULE, &registry(BOOK_ROW), &files), []);
}

#[test]
fn controllers_and_rows_must_match_one_to_one() {
    let stray = "src/modules/public/shelf/controller.ts";
    let two = "src/modules/internal/pair/controller.ts";
    let rows_text = format!(
        "{BOOK_ROW}, {}, {}",
        BOOK_ROW
            .replace("internal/book", "internal/pair")
            .replace("InternalBook", "InternalPair"),
        BOOK_ROW
            .replace("internal/book", "internal/gone")
            .replace("InternalBook", "InternalGone")
    );
    let files = [
        (ENTITY, ENTITY_TEXT),
        (SERVICE, SERVICE_TEXT),
        (CONTROLLER, CONTROLLER_TEXT),
        (stray, "export class PublicShelfController {}\n"),
        (
            two,
            "export class InternalPairController {}\nexport class ExtraController {}\n",
        ),
    ];
    let found = check(RULE, &registry(&rows_text), &files);

    assert_eq!(
        found,
        [
            finding(
                "registration",
                "src/modules/internal/pair/controller.ts",
                None,
                "src/modules/internal/pair/controller.ts must declare exactly one registered controller InternalPairController"
            ),
            finding(
                "registration",
                "src/modules/public/shelf/controller.ts",
                Some(1),
                "src/modules/public/shelf/controller.ts:1 PublicShelfController is an unregistered controller semantic owner"
            ),
            finding(
                "registration",
                "gate.toml",
                None,
                "src/modules/internal/gone/controller.ts is a stale semantic-owner registry row for InternalGoneController"
            ),
        ]
    );
}

#[test]
fn rows_are_checked_against_the_registry_contract() {
    let rows_text = r#"
        { "path": "src/modules/internal/book/routes.ts", "controller-name": "Book", "classification": "OTHER", "entity-name": "BookEntity", "entity-path": "x" },
        { "path": "src/modules/internal/shelf/controller.ts", "controller-name": "InternalShelfController", "classification": "EXTERNAL_NOUN", "primary-service-name": "ShelfService", "primary-service-path": "s", "entity-name": "ShelfEntity", "entity-path": "e" },
        { "path": "src/modules/internal/note/controller.ts", "controller-name": "InternalNoteController", "classification": "PLAIN", "primary-service-name": "NoteService", "primary-service-path": "s", "entity-name": "NoteEntity", "entity-path": null, "semantic-owner-name": "Note", "decision-id": "D-1" }
    "#;
    let found = check(RULE, &registry(rows_text), &[]);

    assert_eq!(
        found,
        [
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/book/routes.ts\" must provide an exact controller.ts path"
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/book/routes.ts\" must provide a Controller class name"
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/book/routes.ts\" must provide a primary Service class name"
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/book/routes.ts\" has unsupported classification \"OTHER\""
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/shelf/controller.ts\" requires an exact decisionId for EXTERNAL_NOUN"
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/shelf/controller.ts\" classifies an external noun outside src/modules/public"
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/note/controller.ts\" semanticOwnerName must identify a parent controller family"
            ),
            finding(
                "registry-contract",
                "gate.toml",
                None,
                "semantic-owner controller contract \"src/modules/internal/note/controller.ts\" has an invalid entityName for PLAIN"
            ),
            finding(
                "registration",
                "gate.toml",
                None,
                "src/modules/internal/book/routes.ts is a stale semantic-owner registry row for Book"
            ),
            finding(
                "registration",
                "gate.toml",
                None,
                "src/modules/internal/shelf/controller.ts is a stale semantic-owner registry row for InternalShelfController"
            ),
            finding(
                "registration",
                "gate.toml",
                None,
                "src/modules/internal/note/controller.ts is a stale semantic-owner registry row for InternalNoteController"
            ),
        ]
    );
}

#[test]
fn metadata_service_handlers_and_contracts_are_checked() {
    let controller = "import { ApiController, IApiControllerBase } from \"@elsikora/nestjs-crud-automator\";\n\
                      import { Controller, Get } from \"@nestjs/common\";\n\
                      import { BookEntity, ShelfEntity } from \"../../domain/book/entity\";\n\
                      import { BookService } from \"../../domain/book/service\";\n\
                      import { IBookFilter, IShelfFilter } from \"./filter.interface\";\n\
                      @ApiController<ShelfEntity>({ entity: ShelfEntity, name: \"Book\" }) @Controller(\"book\")\n\
                      export class InternalBookController {\n\
                      \x20 constructor(public books: BookService) {}\n\
                      \x20 @Get()\n\
                      \x20 public list(filter: IShelfFilter): void {}\n\
                      \x20 public helper(filter: IBookFilter): void {}\n\
                      }\n";
    let files = [
        (ENTITY, ENTITY_TEXT),
        (SERVICE, SERVICE_TEXT),
        (CONTROLLER, controller),
        (
            "src/modules/internal/book/filter.interface.ts",
            "export interface IBookFilter {}\nexport interface IShelfFilter {}\n",
        ),
    ];
    let found = check(RULE, &registry(BOOK_ROW), &files);

    assert_eq!(
        found,
        [
            finding(
                "api-metadata",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController must be one exact ApiController"
            ),
            finding(
                "service-owner",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController primary controller service member must be named service"
            ),
            finding(
                "route-handler-only",
                CONTROLLER,
                Some(11),
                "src/modules/internal/book/controller.ts:11 InternalBookController.helper is not a route-decorated controller handler; move helper logic to its semantic owner"
            ),
            finding(
                "owned-contract-prefix",
                CONTROLLER,
                Some(9),
                "src/modules/internal/book/controller.ts:9 InternalBookController.list exposes owned contract IShelfFilter; expected the IInternalBook semantic-owner prefix"
            ),
            finding(
                "owned-contract-prefix",
                CONTROLLER,
                Some(11),
                "src/modules/internal/book/controller.ts:11 InternalBookController.helper exposes owned contract IBookFilter; expected the IInternalBook semantic-owner prefix"
            ),
            finding(
                "api-metadata",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController binds ApiController entity ShelfEntity; expected BookEntity from src/modules/domain/book/entity.ts, received src/modules/domain/book/entity.ts"
            ),
            finding(
                "api-metadata",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController must implement IApiControllerBase<BookEntity> exactly once"
            ),
            finding(
                "api-metadata",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController config entity must be BookEntity"
            ),
            finding(
                "api-metadata",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController config name must be \"InternalBook\""
            ),
            finding(
                "service-owner",
                CONTROLLER,
                Some(6),
                "src/modules/internal/book/controller.ts:6 InternalBookController and primary service BookService resolve BookEntity from different source owners"
            ),
        ]
    );
}

#[test]
fn a_service_of_another_entity_needs_a_closed_custom_controller() {
    let row_text = r#"{
        "path": "src/modules/internal/book/controller.ts",
        "controller-name": "InternalBookController",
        "classification": "CUSTOM_USE_CASE",
        "primary-service-name": "BookService",
        "primary-service-path": "src/modules/domain/book/service.ts",
        "entity-name": "ShelfEntity",
        "entity-path": "src/modules/domain/book/entity.ts",
        "primary-service-entity-name": "BookEntity",
        "primary-service-entity-path": "src/modules/domain/book/entity.ts",
        "decision-id": "D-SHELF-1"
    }"#;
    let controller = "import { ApiController, ApiRouteCustom, IApiControllerBase } from \"@elsikora/nestjs-crud-automator\";\n\
                      import { ShelfEntity } from \"../../domain/book/entity\";\n\
                      import { BookEntity } from \"../../domain/book/entity\";\n\
                      import { BookService } from \"../../domain/book/service\";\n\
                      @ApiController<ShelfEntity>({ entity: ShelfEntity, name: \"InternalBook\", relations: {} })\n\
                      export class InternalBookController implements IApiControllerBase<BookEntity> {\n\
                      \x20 constructor(public service: BookService) {}\n\
                      \x20 @ApiRouteCustom({ relations: {} })\n\
                      \x20 public shelve(): void {}\n\
                      }\n";
    let files = [
        (ENTITY, ENTITY_TEXT),
        (SERVICE, SERVICE_TEXT),
        (CONTROLLER, controller),
    ];
    let found = check(RULE, &registry(row_text), &files);

    assert_eq!(
        found,
        [
            finding(
                "custom-service-entity",
                CONTROLLER,
                Some(5),
                "src/modules/internal/book/controller.ts:5 InternalBookController a distinct primary service entity requires all six generated routes explicitly disabled"
            ),
            finding(
                "custom-service-entity",
                CONTROLLER,
                Some(5),
                "src/modules/internal/book/controller.ts:5 InternalBookController a distinct primary service entity requires explicit metadata without relation loading"
            ),
            finding(
                "custom-service-entity",
                CONTROLLER,
                Some(5),
                "src/modules/internal/book/controller.ts:5 InternalBookController a distinct primary service entity requires explicit ApiRouteCustom metadata without relation loading"
            ),
        ]
    );
}

#[test]
fn the_registry_file_names_and_planes_are_options() {
    let gone = r#"{ "path": "src/modules/internal/gone/controller.ts", "controller-name": "InternalGoneController", "classification": "EXTERNAL_NOUN", "primary-service-name": "GoneService", "primary-service-path": "s", "entity-name": "GoneEntity", "entity-path": "e", "decision-id": "D-GONE-1" }"#;
    let options = format!(
        "{{\n\"controllers\": [{BOOK_ROW}, {gone}],\n\"service-member\": \"books\",\n\"nest-route-decorators\": [\"Post\"],\n\"external-noun-plane\": \"internal\"\n}}"
    );
    let file = "gate/options/controller-owners.json";
    let files = [
        (ENTITY, ENTITY_TEXT),
        (SERVICE, SERVICE_TEXT),
        (CONTROLLER, CONTROLLER_TEXT),
        (file, options.as_str()),
    ];
    let found = rows(&run_from(RULE, &options, Some(file), "{}", &files).expect("the rule runs"));

    assert_eq!(
        found,
        [
            finding(
                "service-owner",
                CONTROLLER,
                Some(5),
                "src/modules/internal/book/controller.ts:5 InternalBookController primary controller service member must be named books"
            ),
            finding(
                "route-handler-only",
                CONTROLLER,
                Some(8),
                "src/modules/internal/book/controller.ts:8 InternalBookController.list is not a route-decorated controller handler; move helper logic to its semantic owner"
            ),
            finding(
                "registration",
                file,
                None,
                "src/modules/internal/gone/controller.ts is a stale semantic-owner registry row for InternalGoneController"
            ),
        ]
    );

    let message = error(RULE, r#"{ "automator-route-pattern": "(" }"#, &[]);
    assert!(message.contains("automator-route-pattern"), "{message}");
}

#[test]
fn a_row_key_the_rule_does_not_know_is_an_error() {
    let message = error(
        RULE,
        &registry(
            r#"{ "path": "src/modules/internal/book/controller.ts", "controllerName": "X" }"#,
        ),
        &[],
    );
    assert!(
        message.contains("unknown field `controllerName`"),
        "{message}"
    );
    let message = error(RULE, &registry(r#"{ "decision-id": 7 }"#), &[]);
    assert!(message.contains("invalid type: integer `7`"), "{message}");
}

#[test]
fn a_plain_row_without_its_entity_fields_differs_from_null() {
    let plain = |entity: &str| {
        registry(&format!(
            r#"{{ "path": "src/modules/internal/note/controller.ts", "controller-name": "InternalNoteController", "classification": "PLAIN", "primary-service-name": "NoteService", "primary-service-path": "s"{entity} }}"#
        ))
    };
    let invalid = |found: &[Row]| {
        found
            .iter()
            .filter(|row| row.3.contains("invalid entity"))
            .count()
    };

    assert_eq!(
        invalid(&check(
            RULE,
            &plain(r#", "entity-name": null, "entity-path": null"#),
            &[]
        )),
        0
    );
    assert_eq!(invalid(&check(RULE, &plain(""), &[])), 1);
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let generated = "src/modules/public/generated/shelf/controller.ts";
    let vendored = "src/modules/public/node_modules/shelf/controller.ts";
    let files = [
        (ENTITY, ENTITY_TEXT),
        (SERVICE, SERVICE_TEXT),
        (CONTROLLER, CONTROLLER_TEXT),
        (generated, "export class PublicShelfController {}\n"),
        (vendored, "export class PublicStackController {}\n"),
    ];
    let options = registry(BOOK_ROW);
    let paths = |languages: &str| support::paths_with(RULE, &options, "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

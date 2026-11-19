//! `nestjs/automator-metadata` against synthetic entities, DTOs and controllers.

mod support;

use support::{Row, check, error, row, rows, run_from};

const RULE: &str = "automator-metadata";
const ENTITY: &str = "src/modules/domain/book/entity/entity.ts";
const DTO: &str = "src/modules/internal/book/dto/request.dto.ts";
const CONTROLLER: &str = "src/modules/internal/book/controller.ts";

const CLEAN_ENTITY: &str = "import { Entity as Table, Column as Stored, ManyToOne } from \"typeorm\";\n\
                            import * as api from \"@elsikora/nestjs-crud-automator\";\n\
                            import { ApiPropertyDescribe as Describe, EApiPropertyDescribeType } from \"@elsikora/nestjs-crud-automator\";\n\
                            import { Column as LocalColumn } from \"./column\";\n\
                            @Table() export class BookEntity {\n\
                            \x20@LocalColumn() local!: string;\n\
                            \x20@Stored() @Describe({ type: EApiPropertyDescribeType.STRING }) title!: string;\n\
                            \x20@ManyToOne(() => BookEntity) @api.ApiPropertyDescribe({ type: api.EApiPropertyDescribeType.RELATION }) parent!: BookEntity;\n\
                            \x20plain!: string;\n\
                            }\n\
                            class Helper { @Stored() title!: string; }\n";

const CLEAN_DTO: &str = "import { ApiPropertyString as Text } from \"@elsikora/nestjs-crud-automator\";\n\
                         import * as api from \"@elsikora/nestjs-crud-automator\";\n\
                         export class BookDTO {\n\
                         \x20@Text({}) title!: string;\n\
                         \x20@api.ApiPropertyNumber({}) pages!: number;\n\
                         }\n\
                         export class BookHelper { title!: string; }\n";

const CLEAN_CONTROLLER: &str = "import { ApiController, ApiControllerObservable, ApiRouteCustom } from \"@elsikora/nestjs-crud-automator\";\n\
                                import { ApiTags } from \"@nestjs/swagger\";\n\
                                import { Controller } from \"@nestjs/common\";\n\
                                import * as fastify from \"@nestjs/platform-fastify\";\n\
                                const configuration = { routes: { DELETE: { generation: { isEnabled: false } } } };\n\
                                @ApiController(configuration) @ApiControllerObservable() @ApiTags(\"book\") @Controller(\"book\")\n\
                                export class BookController {\n\
                                \x20constructor() {}\n\
                                \x20@ApiRouteCustom({}) @fastify.RouteConfig({}) execute(): void {}\n\
                                \x20get title(): string { return \"\"; }\n\
                                }\n";

fn finding(aspect: &str, path: &str, line: u32, message: &str) -> Row {
    row(aspect, path, Some(line), message)
}

fn contract(
    aspect: &str,
    path: &str,
    line: u32,
    inventory: &str,
    key: &str,
    expected: &str,
    received: &str,
) -> Row {
    finding(
        aspect,
        path,
        line,
        &format!("automator metadata {inventory} {key} expected {expected}; received {received}"),
    )
}

const DESCRIBED: &str = r#"{"describeCount":1,"state":"DESCRIBED_EXPLICIT"}"#;
const ONE_DECORATOR: &str = r#"{"decoratorCount":1,"duplicateValidatorCount":0}"#;
const ONE_OWNER: &str = r#"{"ownerLocalExceptions":[],"owners":[{"kind":"API_ROUTE_CUSTOM"}]}"#;

#[test]
fn described_entities_single_dto_decorators_and_classified_routes_pass() {
    let files = [
        (ENTITY, CLEAN_ENTITY),
        (DTO, CLEAN_DTO),
        (CONTROLLER, CLEAN_CONTROLLER),
    ];

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn entity_properties_need_one_explicit_description() {
    let entity = "import { Entity, Column, OneToMany } from \"typeorm\";\n\
                  import { ApiPropertyDescribe, ApiPropertyString } from \"@elsikora/nestjs-crud-automator\";\n\
                  @Entity() export class BookEntity {\n\
                  \x20@Column() title!: string;\n\
                  \x20@Column() @ApiPropertyDescribe({}) @ApiPropertyDescribe({}) twice!: string;\n\
                  \x20@OneToMany(() => BookEntity, (book) => book.parent) @ApiPropertyString({}) children!: BookEntity[];\n\
                  }\n";
    let entities = |line, key, received| {
        contract(
            "entity-description",
            ENTITY,
            line,
            "entityProperties",
            key,
            DESCRIBED,
            received,
        )
    };

    assert_eq!(
        check(RULE, "{}", &[(ENTITY, entity)]),
        [
            finding(
                "entity-property-decorator",
                ENTITY,
                6,
                "BookEntity.children uses @ApiPropertyString in an entity class; describe entity \
                 fields with @ApiPropertyDescribe only"
            ),
            entities(
                4,
                "BookEntity.title",
                r#"{"describeCount":0,"state":"MISSING"}"#
            ),
            entities(
                5,
                "BookEntity.twice",
                r#"{"describeCount":2,"state":"DESCRIBED_IMPLICIT"}"#
            ),
            entities(
                6,
                "BookEntity.children",
                r#"{"describeCount":0,"state":"MISSING"}"#
            ),
        ]
    );
}

#[test]
fn dto_properties_need_one_automator_decorator_and_no_duplicate_validator() {
    let dto = "import * as api from \"@elsikora/nestjs-crud-automator\";\n\
               import { IsString } from \"class-validator\";\n\
               import { ApiPropertyString as Foreign } from \"@nestjs/swagger\";\n\
               export class BookDTO {\n\
               \x20title!: string;\n\
               \x20@api.ApiPropertyString({}) @api.ApiPropertyString({}) twice!: string;\n\
               \x20@api.ApiPropertyString({}) @IsString() validated!: string;\n\
               \x20@Foreign({}) foreign!: string;\n\
               }\n";
    let dtos = |line, key, received| {
        contract(
            "dto-decorator",
            DTO,
            line,
            "dtoProperties",
            key,
            ONE_DECORATOR,
            received,
        )
    };

    assert_eq!(
        check(RULE, "{}", &[(DTO, dto)]),
        [
            dtos(
                5,
                "BookDTO.title",
                r#"{"decoratorCount":0,"duplicateValidatorCount":0}"#
            ),
            dtos(
                6,
                "BookDTO.twice",
                r#"{"decoratorCount":2,"duplicateValidatorCount":0}"#
            ),
            dtos(
                7,
                "BookDTO.validated",
                r#"{"decoratorCount":1,"duplicateValidatorCount":1}"#
            ),
            dtos(
                8,
                "BookDTO.foreign",
                r#"{"decoratorCount":0,"duplicateValidatorCount":0}"#
            ),
        ]
    );
}

#[test]
fn handlers_need_one_owner_and_classified_decorators() {
    let controller = "import { ApiController, ApiRouteCustom } from \"@elsikora/nestjs-crud-automator\";\n\
                      import { Controller, Get, Body, UseGuards } from \"@nestjs/common\";\n\
                      import { HealthCheck } from \"@nestjs/terminus\";\n\
                      @ApiController({}) @Controller(\"book\") @UseGuards()\n\
                      export class BookController {\n\
                      \x20@Get() list(): void {}\n\
                      \x20@ApiRouteCustom({}) @ApiRouteCustom({}) twice(): void {}\n\
                      \x20@ApiRouteCustom({}) @UseGuards() guarded(): void {}\n\
                      \x20@ApiRouteCustom({}) create(@Body() body: string): void {}\n\
                      \x20@ApiRouteCustom({}) @HealthCheck() health(): void {}\n\
                      }\n";
    let routes = |line, key, received| {
        contract(
            "route-contract",
            CONTROLLER,
            line,
            "routes",
            key,
            ONE_OWNER,
            received,
        )
    };
    let two = r#"[{"kind":"API_ROUTE_CUSTOM"},{"kind":"API_ROUTE_CUSTOM"}]"#;

    assert_eq!(
        check(RULE, "{}", &[(CONTROLLER, controller)]),
        [
            finding(
                "controller-decorator",
                CONTROLLER,
                4,
                &format!(
                    "{CONTROLLER} controller BookController has unclassified class decorator UseGuards"
                )
            ),
            routes(
                6,
                "BookController.list",
                r#"{"ownerLocalExceptions":["Get"],"owners":[]}"#
            ),
            routes(
                7,
                "BookController.twice",
                &format!(r#"{{"ownerLocalExceptions":[],"owners":{two}}}"#)
            ),
            routes(
                8,
                "BookController.guarded",
                r#"{"ownerLocalExceptions":["UseGuards"],"owners":[{"kind":"API_ROUTE_CUSTOM"}]}"#
            ),
            routes(
                10,
                "BookController.health",
                r#"{"ownerLocalExceptions":["HealthCheck"],"owners":[{"kind":"API_ROUTE_CUSTOM"}]}"#
            ),
            finding(
                "route-owner",
                CONTROLLER,
                6,
                "automator metadata routes BookController.list must have exactly one owner; received []"
            ),
            finding(
                "route-owner",
                CONTROLLER,
                7,
                &format!(
                    "automator metadata routes BookController.twice must have exactly one owner; received {two}"
                )
            ),
        ]
    );
}

#[test]
fn options_classify_exceptions_and_stale_entries_are_reported_in_the_options_file() {
    let options = r#"{
  "hidden-properties": {
    "BookEntity.secret": { "type": "EApiPropertyDescribeType.STRING", "reason": "internal" },
    "BookEntity.missing": { "type": "EApiPropertyDescribeType.STRING", "reason": "gone" }
  },
  "parameter-decorators": {
    "BookController.create": { "parameters": [["Body"]], "reason": "raw body" },
    "BookController.update": { "parameters": [["Body"]], "reason": "raw body" },
    "BookController.missing": { "parameters": [], "reason": "gone" }
  },
  "health-check-route": "BookController.health"
}"#;
    let entity = "import { Entity, Column } from \"typeorm\";\n\
                  import { ApiPropertyDescribe, EApiPropertyDescribeType as T } from \"@elsikora/nestjs-crud-automator\";\n\
                  @Entity() export class BookEntity {\n\
                  \x20@Column() @ApiPropertyDescribe({ type: T.STRING }) title!: string;\n\
                  \x20@Column() @ApiPropertyDescribe({ type: T.STRING, isAutoDtoEnabled: false }) secret!: string;\n\
                  }\n";
    let controller = "import { ApiController, ApiRouteCustom } from \"@elsikora/nestjs-crud-automator\";\n\
                      import { Controller, Body, Query } from \"@nestjs/common\";\n\
                      import { HealthCheck } from \"@nestjs/terminus\";\n\
                      @ApiController({}) @Controller(\"book\")\n\
                      export class BookController {\n\
                      \x20@ApiRouteCustom({}) create(@Body() body: string): void {}\n\
                      \x20@ApiRouteCustom({}) @HealthCheck() health(): void {}\n\
                      \x20@ApiRouteCustom({}) update(@Query() query: string): void {}\n\
                      }\n";
    let file = "gate/options/automator-metadata.json";
    let files = [(ENTITY, entity), (CONTROLLER, controller), (file, options)];
    let found = rows(&run_from(RULE, options, Some(file), "{}", &files).expect("the rule runs"));

    let stale = |line, name, key, inventory| {
        finding(
            "policy-key",
            file,
            line,
            &format!(
                "automator metadata policy {name} entry {key} has no matching {inventory} source \
                 entry"
            ),
        )
    };

    assert_eq!(
        found,
        [
            contract(
                "route-contract",
                CONTROLLER,
                8,
                "routes",
                "BookController.update",
                r#"{"ownerLocalExceptions":[],"owners":[{"kind":"API_ROUTE_CUSTOM"}],"parameterDecorators":[["Body"]]}"#,
                r#"{"ownerLocalExceptions":[],"owners":[{"kind":"API_ROUTE_CUSTOM"}],"parameterDecorators":[["Query"]]}"#
            ),
            stale(
                4,
                "hiddenProperties",
                "BookEntity.missing",
                "entityProperties"
            ),
            stale(9, "parameterDecorators", "BookController.missing", "routes"),
        ]
    );
}

#[test]
fn stale_entries_without_an_options_file_are_reported_at_the_configuration() {
    let options = r#"{ "buffer-properties": { "BookEntity.blob": { "reason": "raw bytes" } } }"#;

    assert_eq!(
        check(RULE, options, &[]),
        [row(
            "policy-key",
            "gate.toml",
            None,
            "automator metadata policy bufferProperties entry BookEntity.blob has no matching \
             entityProperties source entry"
        )]
    );

    let message = error(RULE, r#"{ "baseDepth": 1 }"#, &[]);
    assert!(message.contains("baseDepth"), "{message}");
}

#[test]
fn the_walk_reads_files_in_skipped_folders() {
    let dto = |class: &str| format!("export class {class} {{\n\x20title!: string;\n}}\n");
    let built = "src/modules/dist/book/dto/request.dto.ts";
    let generated = "src/modules/generated/book/dto/request.dto.ts";
    let (built_text, generated_text) = (dto("BuiltDTO"), dto("BookDTO"));
    let files = [
        (built, built_text.as_str()),
        (generated, generated_text.as_str()),
    ];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [built, generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [built, generated]);
}

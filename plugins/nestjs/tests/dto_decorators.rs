//! `nestjs/dto-decorators` against synthetic DTO, entity and service files.

mod support;

use support::{check, error, row, rows, run};

const RULE: &str = "dto-decorators";
const DTO: &str = "src/modules/user/dto/create.dto.ts";
const SERVICE: &str = "src/modules/user/user.service.ts";
const AUTOMATOR: &str = "@scope/automator";

fn plugin() -> String {
    format!(r#"{{ "layout": {{ "automator-package": "{AUTOMATOR}" }} }}"#)
}

fn describe(files: &[(&str, &str)]) -> Vec<support::Row> {
    rows(&run(RULE, "{}", &plugin(), files).expect("the rule runs"))
}

#[test]
fn a_dto_that_imports_swagger_property_decorators_is_reported() {
    let text =
        "import { ApiPropertyOptional } from \t'@nestjs/swagger';\nexport class CreateDto {}\n";

    assert_eq!(
        check(RULE, "{}", &[(DTO, text)]),
        [row(
            "swagger-decorator",
            DTO,
            None,
            &format!("{DTO} imports Nest Swagger ApiProperty decorators in a DTO folder")
        )]
    );
}

#[test]
fn the_swagger_test_needs_both_patterns_as_raw_text() {
    let unspaced = "import { ApiProperty } from\"@nestjs/swagger\";\n";
    let prefixed = "import { MyApiProperty } from \"@nestjs/swagger\";\n";
    let other = "import { ApiProperty } from \"@scope/openapi\";\n";
    let outside = "import { ApiProperty } from \"@nestjs/swagger\";\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("src/modules/user/dto/a.dto.ts", unspaced),
                ("src/modules/user/dto/b.dto.ts", prefixed),
                ("src/modules/user/dto/c.dto.ts", other),
                ("src/modules/user/user.controller.ts", outside),
                ("src/shared/dto/d.dto.ts", outside),
            ]
        ),
        []
    );
}

#[test]
fn the_describe_decorator_outside_an_entity_folder_is_reported_once() {
    let text = format!(
        "import {{ ApiPropertyDescribe as Describe }} from \"{AUTOMATOR}\";\n\
         @Describe({{}})\n\
         export class A {{\n\
         \x20 @Describe({{}}) name!: string;\n\
         }}\n"
    );

    assert_eq!(
        describe(&[(SERVICE, &text)]),
        [row(
            "describe-outside-entity",
            SERVICE,
            Some(2),
            &format!("{SERVICE} uses ApiPropertyDescribe outside an entity folder")
        )]
    );
}

#[test]
fn of_two_unrelated_decorated_nodes_the_later_is_reported() {
    let text = format!(
        "import * as automator from \"{AUTOMATOR}\";\n\
         export class A {{\n\
         \x20 @automator.ApiPropertyDescribe({{}}) first!: string;\n\
         \x20 @(automator[\"ApiPropertyDescribe\"])({{}}) second!: string;\n\
         }}\n"
    );

    assert_eq!(
        describe(&[(SERVICE, &text)]),
        [row(
            "describe-outside-entity",
            SERVICE,
            Some(4),
            &format!("{SERVICE} uses ApiPropertyDescribe outside an entity folder")
        )]
    );
}

#[test]
fn entity_files_and_other_packages_are_not_reported() {
    let entity = format!(
        "import {{ ApiPropertyDescribe }} from \"{AUTOMATOR}\";\n\
         export class UserEntity {{ @ApiPropertyDescribe({{}}) name!: string; }}\n"
    );
    let other = "import { ApiPropertyDescribe } from \"@scope/other\";\n\
                 export class A { @ApiPropertyDescribe({}) name!: string; }\n";

    assert_eq!(
        describe(&[
            ("src/modules/user/entity/user.entity.ts", &entity),
            (SERVICE, other),
        ]),
        []
    );
}

#[test]
fn the_folders_and_names_are_options() {
    let options = r#"{ "dto-folder": "input", "swagger-property-word": "ApiField" }"#;
    let text = "import { ApiField } from \"@nestjs/swagger\";\n";

    assert_eq!(
        check(
            RULE,
            options,
            &[("src/modules/user/input/a.ts", text), (DTO, text)]
        ),
        [row(
            "swagger-decorator",
            "src/modules/user/input/a.ts",
            None,
            "src/modules/user/input/a.ts imports Nest Swagger ApiField decorators in a DTO folder"
        )]
    );

    let message = error(RULE, r#"{ "dto_folder": "/x/" }"#, &[]);
    assert!(message.contains("dto_folder"), "{message}");
}

#[test]
fn a_folder_option_with_a_slash_is_an_error() {
    let message = error("dto-decorators", r#"{ "dto-folder": "dto/" }"#, &[]);
    assert!(
        message
            .contains("option `dto-folder` is \"dto/\", which is not a plain name; write \"dto\""),
        "{message}"
    );
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let swagger = "import { ApiProperty } from \"@nestjs/swagger\";\nexport class CreateDto {}\n";
    let decorated = format!(
        "import {{ ApiPropertyDescribe }} from \"{AUTOMATOR}\";\n\
         export class A {{ @ApiPropertyDescribe({{}}) name!: string; }}\n"
    );
    let generated_dto = "src/modules/user/generated/dto/create.dto.ts";
    let generated_service = "src/modules/user/generated/user.service.ts";
    let vendored_dto = "src/modules/node_modules/pkg/dto/create.dto.ts";
    let vendored_service = "src/modules/node_modules/pkg/user.service.ts";
    let files = [
        (generated_dto, swagger),
        (generated_service, decorated.as_str()),
        (vendored_dto, swagger),
        (vendored_service, decorated.as_str()),
    ];
    let plugin_options = plugin();
    let paths =
        |languages: &str| support::paths_with(RULE, "{}", &plugin_options, languages, &files);

    assert_eq!(paths("{}"), [generated_dto, generated_service]);
    assert_eq!(
        paths(support::SKIP_GENERATED),
        [vendored_dto, vendored_service]
    );
}

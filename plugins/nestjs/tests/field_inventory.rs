//! `nestjs/field-inventory` against synthetic entities, controllers, DTOs and contracts.

mod support;

use gate_sdk::serde_json::{self, Value};
use support::{Row, error, row, rows, run_from};

const RULE: &str = "field-inventory";
const ENTITY: &str = "src/modules/domain/user/entity/user.entity.ts";
const CONTROLLER: &str = "src/modules/internal/user/controller.ts";
const PROFILE: &str = "src/modules/public/user/dto/profile.dto.ts";
const OPTIONS_FILE: &str = "gate/options/nestjs/field-inventory.json";

const IMPORTS: &str = "import { Column, Entity, PrimaryGeneratedColumn, Unique } from \"typeorm\";\n\
                       import { ApiPropertyDescribe, EApiPropertyDescribeType, EApiPropertyStringType } from \"@elsikora/nestjs-crud-automator\";\n";

fn entity(unique: &str, code_pattern: &str) -> String {
    format!(
        "{IMPORTS}@Entity()\n\
         {unique}\n\
         export class UserEntity {{\n\
         \x20 @PrimaryGeneratedColumn(\"uuid\") id!: string;\n\
         \x20 @Column({{ type: \"varchar\" }}) @ApiPropertyDescribe({{ type: EApiPropertyDescribeType.STRING, format: EApiPropertyStringType.STRING{code_pattern} }}) code!: string;\n\
         \x20 @Column({{ type: \"text\" }}) @ApiPropertyDescribe({{ type: EApiPropertyDescribeType.STRING, format: EApiPropertyStringType.STRING }}) bio!: string;\n\
         \x20 @Column({{ type: \"varchar\" }}) @ApiPropertyDescribe({{ type: EApiPropertyDescribeType.STRING, format: EApiPropertyStringType.EMAIL }}) email!: string;\n\
         \x20 @Column({{ type: \"integer\" }}) age!: number;\n\
         }}\n"
    )
}

const KEBAB: &str = ", pattern: \"/^[a-z0-9]+(-[a-z0-9]+)*$/\"";

fn controller(routes: &str) -> String {
    format!(
        "import {{ ApiController }} from \"@elsikora/nestjs-crud-automator\";\n\
         import {{ UserEntity }} from \"../../domain/user/entity/user.entity\";\n\
         @ApiController({{ entity: UserEntity, routes: {{ {routes} }} }})\n\
         export class UserController {{}}\n"
    )
}

const READ_ONLY: &str = "UPDATE: { generation: { isEnabled: false } }, PARTIAL_UPDATE: { generation: { isEnabled: false } }";

const PROFILE_DTO: &str = "import { ApiPropertyString, EApiPropertyStringType } from \"@elsikora/nestjs-crud-automator\";\n\
                           export class ProfileDTO {\n\
                           \x20 @ApiPropertyString({ format: EApiPropertyStringType.STRING }) nickname!: string;\n\
                           \x20 @ApiPropertyString({ format: EApiPropertyStringType.UUID }) token!: string;\n\
                           }\n";

const FIELDS: &str = r#"{
  "fields": [
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "code",
      "class": "CODE", "kind": "STABLE_KEY", "role": "IDENTITY", "scope": [], "decision": "D-0001", "exposure": ["INTERNAL"] },
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "bio",
      "class": "AUTHORED_CONTENT", "decision": "D-0001", "exposure": ["INTERNAL"] },
    { "path": "src/modules/public/user/dto/profile.dto.ts", "class-name": "ProfileDTO", "member": "nickname",
      "class": "PLAYER_CONTENT", "decision": "D-0001", "exposure": ["PUBLIC"] }
  ]
}"#;

fn clean() -> Vec<(&'static str, String)> {
    vec![
        (ENTITY, entity("@Unique([\"code\"])", KEBAB)),
        (CONTROLLER, controller(READ_ONLY)),
        (PROFILE, PROFILE_DTO.to_owned()),
    ]
}

/// The decision index the checks read unless their options name another one.
const DECISIONS: &str = "| `D-0001` | accepted | Fields | `d1.md` |\n";

/// The rule's findings, with `docs/decisions.md` as the decision index unless `options` names one.
fn check(rule: &str, options: &str, files: &[(&str, &str)]) -> Vec<Row> {
    let mut value: Value = serde_json::from_str(options).expect("the options are JSON");

    if let Some(object) = value.as_object_mut() {
        object
            .entry("decisions")
            .or_insert_with(|| Value::from("docs/decisions.md"));
    }

    let mut all = files.to_vec();

    if !all.iter().any(|(path, _)| *path == "docs/decisions.md") {
        all.push(("docs/decisions.md", DECISIONS));
    }

    support::check(rule, &value.to_string(), &all)
}

fn files<'o>(owned: &'o [(&'static str, String)]) -> Vec<(&'static str, &'o str)> {
    owned
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect()
}

fn at(aspect: &str, path: &str, line: u32, message: &str) -> Row {
    row(aspect, path, Some(line), message)
}

fn entry(aspect: &str, line: Option<u32>, message: &str) -> Row {
    row(aspect, "gate.toml", line, message)
}

fn unclassified(path: &str, line: u32, field: &str) -> Row {
    at(
        "unclassified",
        path,
        line,
        &format!("{field} can hold free text and has no entry in gate.toml"),
    )
}

fn source(message: &str) -> Row {
    entry(
        "config-key-source",
        None,
        &format!("entry UserEntity.code references {message}"),
    )
}

#[test]
fn classified_fields_pass() {
    let owned = clean();
    assert_eq!(check(RULE, FIELDS, &files(&owned)), []);
}

#[test]
fn unclassified_fields_and_exposure_drift_are_reported() {
    let owned = clean();

    assert_eq!(
        check(RULE, "{}", &files(&owned)),
        [
            unclassified(ENTITY, 7, "UserEntity.code"),
            unclassified(ENTITY, 8, "UserEntity.bio"),
            unclassified(PROFILE, 3, "ProfileDTO.nickname"),
        ]
    );

    let targets: Vec<Option<String>> = support::run(RULE, "{}", "{}", &files(&owned))
        .expect("the rule runs")
        .into_iter()
        .map(|finding| finding.target)
        .collect();
    assert_eq!(
        targets,
        [
            Some(format!("{ENTITY}:UserEntity.code [INTERNAL]")),
            Some(format!("{ENTITY}:UserEntity.bio [INTERNAL]")),
            Some(format!("{PROFILE}:ProfileDTO.nickname [PUBLIC]")),
        ]
    );
    let private = FIELDS
        .replace(
            "\"class\": \"AUTHORED_CONTENT\", \"decision\": \"D-0001\"",
            "\"class\": \"STAFF_NOTE\", \"exposing-decision\": \"D-0001\", \"decision\": \"D-0001\"",
        )
        .replace(
            "\"PLAYER_CONTENT\", \"decision\": \"D-0001\", \"exposure\": [\"PUBLIC\"]",
            "\"THIRD_PARTY\", \"decision\": \"D-0001\", \"exposure\": [\"PUBLIC\"]",
        );
    assert_eq!(
        check(RULE, &private, &files(&clean())),
        [
            entry(
                "stale-entry",
                None,
                "entry UserEntity.bio classifies a field that is exposed on INTERNAL only, so its \
                 exposingDecision exposes nothing"
            ),
            at(
                "private-exposure",
                PROFILE,
                3,
                "ProfileDTO.nickname is a THIRD_PARTY exposed on PUBLIC without an exposingDecision"
            ),
        ]
    );

    let drift = FIELDS.replace(
        "\"PLAYER_CONTENT\", \"decision\": \"D-0001\", \"exposure\": [\"PUBLIC\"]",
        "\"PLAYER_CONTENT\", \"decision\": \"D-0001\", \"exposure\": [\"INTERNAL\"]",
    );
    let mut owned = clean();
    owned[1].1 = controller(&format!(
        "{READ_ONLY}, GET: {{ dto: {{ RESPONSE: ProfileDTO }} }}"
    ));
    owned[1].1 = owned[1].1.replace(
        "import { ApiController }",
        "import { ProfileDTO } from \"../../public/user/dto/profile.dto\";\nimport { ApiController }",
    );
    assert_eq!(
        check(RULE, &drift, &files(&owned)),
        [at(
            "exposure",
            PROFILE,
            3,
            "ProfileDTO.nickname is exposed on INTERNAL, PUBLIC but its entry declares INTERNAL"
        )]
    );
}

#[test]
fn entries_are_validated_and_stale_entries_reported() {
    let options = r#"{
  "fields": [
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "code",
      "class": "CODE", "decision": "D-0001", "exposure": ["INTERNAL"] },
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "code",
      "class": "MACHINE", "decision": "D-0001", "exposure": [] },
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "bio",
      "class": "AUTHORED_CONTENT", "kind": "KEBAB", "decision": "D-0009", "exposure": ["INTERNAL", "INTERNAL"],
      "exposing-decision": "D-0001", "alternative": "AUTHORED_CONTENT", "role": "REFERENCE" },
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "email",
      "class": "PERSONAL_DATA", "decision": "D-0001", "exposure": [] },
    { "path": "src/modules/domain/user/entity/user.entity.ts", "class-name": "UserEntity", "member": "gone",
      "class": "MACHINE", "reason": "", "decision": "D-0001", "exposure": [] },
    { "path": "src/modules/public/user/dto/profile.dto.ts", "class-name": "ProfileDTO", "member": "nickname",
      "class": "STAFF_NOTE", "decision": "D-0001", "exposure": ["PUBLIC"], "exposing-decision": "D-0001" }
  ],
  "decisions": "docs/decisions.md"
}"#;
    let mut owned = clean();
    owned.push((OPTIONS_FILE, options.to_owned()));
    owned.push((
        "docs/decisions.md",
        "| `D-0001` | accepted | Fields | `d1.md` |\n| `D-0009` | proposed | Later | `d9.md` |\n"
            .to_owned(),
    ));
    let found = rows(
        &run_from(RULE, options, Some(OPTIONS_FILE), "{}", &files(&owned)).expect("the rule runs"),
    );
    let at_entry =
        |aspect: &str, line, message: &str| row(aspect, OPTIONS_FILE, Some(line), message);
    let bio = |message: &str| {
        at_entry(
            "classification",
            7,
            &format!("entry UserEntity.bio {message}"),
        )
    };

    assert_eq!(
        found,
        [
            at_entry(
                "classification",
                3,
                "entry UserEntity.code is a CODE and needs a kind"
            ),
            at_entry(
                "classification",
                5,
                "entry UserEntity.code repeats an earlier entry for the same field"
            ),
            bio("lists an exposure plane twice"),
            bio("names D-0009, which is not an accepted decision"),
            bio("names a role or a scope, which only a CODE entry carries"),
            bio("names an exposingDecision, which only a STAFF_NOTE or THIRD_PARTY entry carries"),
            bio("names its own class AUTHORED_CONTENT as its alternative"),
            bio("names the code kind KEBAB, which only a CODE entry carries"),
            at_entry(
                "stale-entry",
                10,
                "entry UserEntity.email classifies a field that cannot hold free text: its \
                 Automator format is EMAIL"
            ),
            at_entry(
                "classification",
                12,
                "entry UserEntity.gone is a MACHINE value and needs a reason"
            ),
            at_entry(
                "stale-entry",
                12,
                "entry UserEntity.gone matches no entity column or DTO property"
            ),
        ]
    );
}

#[test]
fn codes_keep_their_pattern_stay_unchanged_and_unique() {
    let mut owned = clean();
    owned[0].1 = entity("", ", pattern: \"/^[A-Z]+$/\"");
    owned[1].1 = controller("");
    let code =
        |aspect: &str, message: &str| at(aspect, ENTITY, 7, &format!("UserEntity.code {message}"));

    assert_eq!(
        check(RULE, FIELDS, &files(&owned)),
        [
            code(
                "code-format",
                "is a code of kind STABLE_KEY that has the Automator pattern /^[A-Z]+$/, not \
                 /^[a-z0-9]+(-[a-z0-9]+)*$/"
            ),
            code(
                "code-immutable",
                "is an IDENTITY code that the generated PARTIAL_UPDATE body of UserController, \
                 the generated UPDATE body of UserController can change"
            ),
            code(
                "code-unique",
                "is an IDENTITY code with no unique index, unique constraint or primary key over \
                 exactly code on UserEntity"
            ),
        ]
    );

    let stable_public = FIELDS.replace("\"role\": \"IDENTITY\", \"scope\": [], \"decision\": \"D-0001\", \"exposure\": [\"INTERNAL\"]", "\"role\": \"IDENTITY\", \"scope\": [], \"decision\": \"D-0001\", \"exposure\": [\"INTERNAL\", \"PUBLIC\"]");
    let mut owned = clean();
    owned.push((
        "src/modules/public/user/controller.ts",
        controller(READ_ONLY).replace("UserController", "PublicUserController"),
    ));
    assert_eq!(
        check(RULE, &stable_public, &files(&owned)),
        [
            code(
                "private-exposure",
                "is a STABLE_KEY code exposed on INTERNAL, PUBLIC, but an internal stable key is \
                 never displayed on PUBLIC"
            ),
            at(
                "exposure",
                ENTITY,
                8,
                "UserEntity.bio is exposed on INTERNAL, PUBLIC but its entry declares INTERNAL"
            ),
        ]
    );
}

#[test]
fn configuration_keys_name_their_contracts() {
    let options = |reference: &str| {
        FIELDS.replace(
            "\"kind\": \"STABLE_KEY\", \"role\": \"IDENTITY\", \"scope\": []",
            &format!("\"kind\": \"CONFIG_KEY\", \"role\": \"IDENTITY\", \"scope\": [], \"reference\": {reference}"),
        )
    };
    let contract = ("src/config/runtime-config/user.config.ts", "export interface IUserConfig { users: { code: string; limit?: string }[]; count: number; }\n".to_owned());
    let mut owned = clean();
    owned.push(contract);
    let good = r#"[{ "source": "src/config/runtime-config/user.config.ts: IUserConfig.users.code", "decision": "D-0001", "reason": "the key names the record" }]"#;
    assert_eq!(check(RULE, &options(good), &files(&owned)), []);

    let bad = r#"[
      { "source": "src/config/runtime-config/user.config.ts: IUserConfig.users.code", "decision": "D-0001", "reason": "" },
      { "source": "src/config/runtime-config/user.config.ts: IUserConfig.users.code", "decision": "D-0001", "reason": "twice" },
      { "source": "src/config/runtime-config/user.config.ts: IUserConfig.count", "decision": "D-0001", "reason": "not text" },
      { "source": "src/other/user.config.ts: IUserConfig.users", "decision": "D-0001", "reason": "outside" },
      { "source": "nothing", "decision": "D-0001", "reason": "malformed" }
    ]"#;
    let users = "\"src/config/runtime-config/user.config.ts: IUserConfig.users.code\"";

    assert_eq!(
        check(RULE, &options(bad), &files(&owned)),
        [
            entry(
                "config-key-source",
                None,
                &format!("entry UserEntity.code lists the reference {users} twice")
            ),
            source("\"nothing\", which is not <path>: <Type>.<member>"),
            source(
                "\"src/config/runtime-config/user.config.ts: IUserConfig.count\", which names \
                 IUserConfig.count, which is not typed string"
            ),
            source(&format!("{users} without a reason")),
            source(
                "\"src/other/user.config.ts: IUserConfig.users\", which lies outside the \
                 configuration and secret roots src/config/runtime-config/, \
                 src/infrastructure/configuration/, infrastructure/cdk/lib/config/"
            ),
        ]
    );
    assert_eq!(
        check(RULE, &options("[]"), &files(&owned)),
        [at(
            "config-key-unreferenced",
            ENTITY,
            7,
            "UserEntity.code is a configuration key that no configuration or secret contract \
             references"
        )]
    );
    let misplaced = FIELDS.replace(
        "\"class\": \"AUTHORED_CONTENT\", \"decision\": \"D-0001\"",
        &format!(
            "\"class\": \"AUTHORED_CONTENT\", \"reference\": {good}, \"decision\": \"D-0001\""
        ),
    );
    assert_eq!(
        check(RULE, &misplaced, &files(&owned)),
        [entry(
            "config-key-field",
            None,
            "entry UserEntity.bio lists references, which only a CODE entry of kind CONFIG_KEY \
             carries"
        )]
    );
}

#[test]
fn unreadable_configuration_planes_and_vocabularies_are_options() {
    let mut owned = clean();
    owned[1].1 = "import { ApiController } from \"@elsikora/nestjs-crud-automator\";\n\
                  import { build } from \"./build\";\n\
                  @ApiController(build())\n\
                  export class UserController {}\n"
        .to_owned();
    let drift = |line, field: &str| {
        at(
            "exposure",
            ENTITY,
            line,
            &format!("UserEntity.{field} is exposed on no plane but its entry declares INTERNAL"),
        )
    };

    assert_eq!(
        check(RULE, FIELDS, &files(&owned)),
        [
            drift(7, "code"),
            drift(8, "bio"),
            at(
                "unreadable",
                CONTROLLER,
                3,
                "UserController: nestjs/field-inventory cannot read the entity, routes or GET_LIST \
                 configuration of this @ApiController, so the planes it exposes are unknown"
            ),
        ]
    );

    // Text columns, machine formats and code formats are options: `bio` is no text column, a
    // UUID admits free text and an e-mail is a code.
    let options = r#"{ "text-column-types": ["varchar"], "machine-formats": [], "code-formats": { "EMAIL": "KEBAB" } }"#;

    assert_eq!(
        check(RULE, options, &files(&clean())),
        [
            unclassified(ENTITY, 7, "UserEntity.code"),
            unclassified(PROFILE, 3, "ProfileDTO.nickname"),
            unclassified(PROFILE, 4, "ProfileDTO.token"),
        ]
    );

    let message = error(RULE, r#"{ "fields": [{ "path": "a" }] }"#, &[]);
    assert!(message.contains("missing field `class-name`"), "{message}");
}

#[test]
fn entries_without_their_decision_index_are_one_finding() {
    let owned = clean();
    let fields: Value = serde_json::from_str(FIELDS).expect("the fields are JSON");
    let missing = format!(
        r#"{{ "fields": {}, "decisions": "docs/missing.md" }}"#,
        fields["fields"]
    );

    assert_eq!(
        support::check(RULE, FIELDS, &files(&owned)),
        [row(
            "classification",
            "gate.toml",
            None,
            "the entries name decisions, but `decisions` names no decision index; set it to the \
             index that lists the accepted decisions"
        )]
    );
    assert_eq!(
        support::check(RULE, &missing, &files(&owned)),
        [row(
            "classification",
            "gate.toml",
            None,
            "the decision index docs/missing.md is not tracked; correct `decisions` or add the index"
        )]
    );
    assert!(
        support::check(RULE, "{}", &files(&owned))
            .iter()
            .all(|found| found.0 != "classification")
    );
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let generated = "src/modules/public/generated/user/dto/profile.dto.ts";
    let vendored = "src/modules/public/node_modules/user/dto/profile.dto.ts";
    let package_dto = PROFILE_DTO.replace("ProfileDTO", "PackageDTO");
    let files = [
        (generated, PROFILE_DTO),
        (vendored, package_dto.as_str()),
        ("docs/decisions.md", DECISIONS),
    ];
    let options = r#"{ "decisions": "docs/decisions.md" }"#;
    let paths = |languages: &str| support::paths_with(RULE, options, "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

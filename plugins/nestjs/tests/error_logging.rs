//! `nestjs/error-logging` against synthetic logger calls and ORM options.

mod support;

use support::{Row, check, error, row};

const RULE: &str = "error-logging";
const SERVICE: &str = "src/modules/user/user.service.ts";
const CONFIG: &str = "src/config/database.config.ts";
const IMPORTS: &str = "import { Logger } from \"@nestjs/common\";\n\
                       import { FormatErrorEvidenceForLog } from \"@elsikora/nestjs-crud-automator\";\n";

fn logger(path: &str, line: u32) -> Row {
    row(
        "logger-error-evidence",
        path,
        Some(line),
        &format!(
            "{path}:{line} native Logger.error must pass FormatErrorEvidenceForLog(actual error \
             evidence) and a bounded message/context, with no raw additional arguments"
        ),
    )
}

fn logging(path: &str, line: u32, message: &str) -> Row {
    row(
        "typeorm-logging-disabled",
        path,
        Some(line),
        &format!("{path}:{line} {message}"),
    )
}

const UNFINISHED: &str = "native TypeORM options must end with effective literal logging:false \
                          after overriding spreads or computed properties";
const NOT_LOCAL: &str = "native TypeORM options factory or constructor must finalize its \
                         effective logging:false options locally on every branch";
const MUTATED: &str = "native TypeORM options logging must not be mutated through a materialized \
                       object or its local aliases";

fn service(body: &str) -> String {
    format!(
        "{IMPORTS}export class UserService {{\n\
         \x20 private readonly logger = new Logger(UserService.name);\n\
         \x20 run(): void {{\n\
         \x20   try {{ this.work(); }} catch (error) {{\n\
         {body}\n\
         \x20   }}\n\
         \x20 }}\n\
         \x20 work(): void {{}}\n\
         }}\n"
    )
}

#[test]
fn formatted_evidence_and_a_bounded_message_pass() {
    let text = service(
        "      this.logger.error(\"user run failed\", FormatErrorEvidenceForLog(error));\n\
         \x20     Logger.error(FormatErrorEvidenceForLog(error), \"user run failed\");",
    );
    let options = "import { DataSource, DataSourceOptions } from \"typeorm\";\n\
                   export const OPTIONS: DataSourceOptions = { type: \"postgres\", logging: false };\n\
                   export const SOURCE = new DataSource({ ...OPTIONS, logging: false });\n";

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, &text), (CONFIG, options)]),
        []
    );
}

#[test]
fn raw_evidence_a_missing_formatter_or_extra_arguments_are_reported() {
    let text = service(
        "      this.logger.error(\"user run failed\", error);\n\
         \x20     Logger.error(error);\n\
         \x20     this.logger.error(\"failed\", FormatErrorEvidenceForLog(error), error);",
    );

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, &text)]),
        [logger(SERVICE, 7), logger(SERVICE, 8), logger(SERVICE, 9)]
    );
}

#[test]
fn orm_options_must_end_with_logging_false_and_stay_unchanged() {
    let text = "import { DataSource, DataSourceOptions } from \"typeorm\";\n\
                import { BASE } from \"./base\";\n\
                export const LATE: DataSourceOptions = { logging: false, ...BASE };\n\
                export const MISSING = new DataSource({ type: \"postgres\" });\n\
                export const FACTORY = new DataSource(BASE);\n\
                const KEPT: DataSourceOptions = { type: \"postgres\", logging: false };\n\
                KEPT.logging = true;\n";

    assert_eq!(
        check(RULE, "{}", &[(CONFIG, text)]),
        [
            logging(CONFIG, 5, NOT_LOCAL),
            logging(CONFIG, 7, MUTATED),
            logging(CONFIG, 3, UNFINISHED),
            logging(CONFIG, 4, UNFINISHED),
        ]
    );
}

#[test]
fn roots_cover_source_code_but_not_other_projects_tests_or_declarations() {
    let text =
        format!("{IMPORTS}export function fail(error: Error): void {{ Logger.error(error); }}\n");
    let files = [
        ("src/node_modules/pkg/index.ts", text.as_str()),
        ("src/user.ts", text.as_str()),
        ("infrastructure/cdk/lib/stack.ts", text.as_str()),
        ("infrastructure/cdk/other/stack.ts", text.as_str()),
        ("test/user.case.ts", text.as_str()),
        ("src/user.d.ts", text.as_str()),
    ];

    assert_eq!(
        check(RULE, "{}", &files),
        [
            logger("src/node_modules/pkg/index.ts", 3),
            logger("src/user.ts", 3),
        ]
    );
}

#[test]
fn the_names_and_roots_are_options() {
    let options = r#"{
        "roots": ["app"],
        "logger-class": "Journal",
        "logger-method": "fail",
        "logging-property": "verbose"
    }"#;
    let journal = "import { Journal } from \"@nestjs/common\";\n\
                   export function fail(error: Error): void { Journal.fail(error); Journal.error(error); }\n";
    let orm = "import { DataSourceOptions } from \"typeorm\";\n\
               export const OPTIONS: DataSourceOptions = { verbose: false, logging: true };\n";

    assert_eq!(
        check(
            RULE,
            options,
            &[("app/a.ts", journal), ("app/b.ts", orm), (SERVICE, journal)]
        ),
        [logger("app/a.ts", 2)]
    );

    let message = error(RULE, r#"{ "loggerClass": "Journal" }"#, &[]);
    assert!(message.contains("loggerClass"), "{message}");
}

#[test]
fn the_walk_reads_files_in_skipped_folders() {
    let text =
        format!("{IMPORTS}export function fail(error: Error): void {{ Logger.error(error); }}\n");
    let built = "src/dist/user.ts";
    let generated = "src/modules/generated/user/user.ts";
    let files = [(built, text.as_str()), (generated, text.as_str())];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [built, generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [built, generated]);
}

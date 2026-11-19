//! `nestjs/native-functions` against synthetic services.

mod host;

use host::{check, error, row};

const RULE: &str = "native-functions";
const PATH: &str = "src/modules/book/book.service.ts";

fn service(body: &str) -> String {
    format!(
        "import {{ ApiService, ApiFunctionCustom, ApiFunctionStep }} from \"@elsikora/nestjs-crud-automator\";\n\
         @ApiService({{ entity: BookEntity }})\n\
         export class BookService {{\n{body}}}\n"
    )
}

fn at(line: u32, text: &str) -> String {
    format!("{PATH}:{line} {text}")
}

#[test]
fn a_well_formed_service_passes() {
    let body = "  @ApiFunctionCustom({ transaction: { mode: \"write\" } })\n\
                \x20 async lend(): Promise<void> {}\n\
                \x20 @ApiFunctionStep({ transaction: { mode: \"read\" } })\n\
                \x20 private async count(): Promise<number> { return 1; }\n\
                \x20 async onModuleInit(): Promise<void> {}\n\
                \x20 async get(): Promise<void> {}\n\
                \x20 name(): string { return \"\"; }\n\
                \x20 private async helper(): Promise<void> {}\n\
                \x20 constructor() {}\n\
                \x20 get title(): string { return \"\"; }\n";

    assert_eq!(check(RULE, "{}", &[(PATH, &service(body))]), []);
}

#[test]
fn undecorated_and_unfit_members_are_reported() {
    let body = "  async lend(): Promise<void> {}\n\
                \x20 find() {}\n\
                \x20 static async make(): Promise<void> {}\n\
                \x20 handle = async () => {};\n";

    assert_eq!(
        check(RULE, "{}", &[(PATH, &service(body))]),
        [
            row(
                "undecorated-method",
                PATH,
                Some(4),
                &at(
                    4,
                    "BookService.lend is a non-private asynchronous method of a native service without ApiFunctionCustom or ApiFunctionStep"
                )
            ),
            row(
                "undecorated-method",
                PATH,
                Some(5),
                &at(
                    5,
                    "BookService.find is a non-private asynchronous method of a native service without ApiFunctionCustom or ApiFunctionStep"
                )
            ),
            row(
                "undecorated-method",
                PATH,
                Some(6),
                &at(
                    6,
                    "BookService.make is a non-private static method of a native service, which cannot be a Crud Automator function"
                )
            ),
            row(
                "undecorated-method",
                PATH,
                Some(7),
                &at(
                    7,
                    "BookService.handle is a non-private function-valued property of a native service, which cannot be a Crud Automator function"
                )
            ),
        ]
    );
}

#[test]
fn decorator_misuse_is_reported_per_decorator() {
    let body = "  @ApiFunctionCustom({ transaction: { mode: \"write\" } })\n\
                \x20 @ApiFunctionStep({})\n\
                \x20 async lend(): Promise<void> {}\n\
                \x20 @ApiFunctionCustom({ transaction: { mode: \"write\" } })\n\
                \x20 private title(): string { return \"\"; }\n";

    assert_eq!(
        check(RULE, "{}", &[(PATH, &service(body))]),
        [
            row(
                "function-decorator-count",
                PATH,
                Some(5),
                &at(
                    5,
                    "BookService.lend carries more than one Crud Automator function decorator"
                )
            ),
            row(
                "transaction-mode",
                PATH,
                Some(5),
                &at(
                    5,
                    "BookService.lend carries ApiFunctionStep without an explicit transaction: { mode }"
                )
            ),
            row(
                "synchronous-function",
                PATH,
                Some(7),
                &at(
                    7,
                    "BookService.title carries ApiFunctionCustom but is no asynchronous method, and the decorator makes every call return a Promise"
                )
            ),
            row(
                "private-custom",
                PATH,
                Some(7),
                &at(
                    7,
                    "BookService.title is private and carries ApiFunctionCustom; a participant inside its owner is an ApiFunctionStep"
                )
            ),
        ]
    );
}

#[test]
fn service_decorators_owners_and_repository_services_are_checked() {
    let text = "import { ApiService, ApiServiceBase, ApiFunctionStep } from \"@elsikora/nestjs-crud-automator\";\n\
                import { InjectRepository } from \"@nestjs/typeorm\";\n\
                @ApiService({})\n\
                export class ShelfService {}\n\
                export class RackService extends ApiServiceBase {}\n\
                export class Plain {\n\
                \x20 @ApiFunctionStep({ transaction: { mode: \"read\" } })\n\
                \x20 async run(): Promise<void> {}\n\
                }\n\
                export class Loose {\n\
                \x20 constructor(@InjectRepository(Shelf) private readonly repository: Repository<Shelf>) {}\n\
                \x20 async load(): Promise<void> {}\n\
                }\n";
    let path = "src/modules/shelf/service.ts";

    assert_eq!(
        check(RULE, "{}", &[(path, text)]),
        [
            row(
                "api-service",
                path,
                Some(3),
                &format!(
                    "{path}:3 ShelfService is a native service without its own Crud Automator ApiService decorator that names its entity"
                )
            ),
            row(
                "api-service",
                path,
                Some(5),
                &format!(
                    "{path}:5 RackService is a native service without its own Crud Automator ApiService decorator that names its entity"
                )
            ),
            row(
                "function-owner",
                path,
                Some(7),
                &format!(
                    "{path}:7 Plain.run carries ApiFunctionStep outside a native Crud Automator service"
                )
            ),
            row(
                "api-service",
                path,
                Some(10),
                &format!(
                    "{path}:10 Loose injects a repository through InjectRepository but is no Crud Automator ApiService"
                )
            ),
            row(
                "undecorated-method",
                path,
                Some(12),
                &format!(
                    "{path}:12 Loose.load is a non-private asynchronous method of a native service without ApiFunctionCustom or ApiFunctionStep"
                )
            ),
        ]
    );
}

#[test]
fn options_rename_the_vocabulary() {
    let text = "import { Service, Job } from \"@scope/crud\";\n\
                @Service({ model: Book })\n\
                export class BookService {\n\
                \x20 @Job({ tx: { kind: 1 } })\n\
                \x20 async lend(): Promise<void> {}\n\
                \x20 async reload(): Promise<void> {}\n\
                }\n";
    let options = r#"{ "framework-package": "@scope/crud", "service-decorator": "Service",
        "function-decorators": ["Job"], "custom-function-decorator": "Job", "entity-option": "model",
        "transaction-option": ["tx", "kind"], "inherited-methods": ["reload"] }"#;

    assert_eq!(check(RULE, options, &[(PATH, text)]), []);
}

#[test]
fn option_errors_are_reported() {
    assert!(error(RULE, r#"{ "function-decorators": [] }"#, &[]).contains("function-decorators"));
    assert!(error(RULE, r#"{ "skip-folders": [] }"#, &[]).contains("skip-folders"));
}

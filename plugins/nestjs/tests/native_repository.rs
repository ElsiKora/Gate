//! `nestjs/native-repository` against synthetic services.

mod host;

use host::{check, error, row};

const RULE: &str = "native-repository";
const AUTOMATOR: &str = "@elsikora/nestjs-crud-automator";

fn message(path: &str, line: u32, class: &str) -> String {
    format!(
        "{path}:{line} {class}.repository must be a protected readonly native repository parameter property"
    )
}

#[test]
fn decorated_based_and_derived_services_need_protected_readonly() {
    let decorated = format!(
        "import * as Crud from \"{AUTOMATOR}\";\n\
         @Crud.ApiService({{ entity: BookEntity }})\n\
         export class BookService {{\n\
         \x20 constructor(\n\
         \x20   @InjectRepository(BookEntity)\n\
         \x20   private readonly repository: Repository<BookEntity>,\n\
         \x20 ) {{}}\n\
         }}\n"
    );
    let based = format!(
        "import {{ ApiServiceBase as Base }} from \"{AUTOMATOR}\";\n\
         export class ShelfService extends Base<ShelfEntity> {{\n\
         \x20 constructor(public repository: Repository<ShelfEntity>) {{ super(); }}\n\
         }}\n"
    );
    let derived = "import { BookService } from \"./book.service\";\n\
                   export class RareBookService extends BookService {\n\
                   \x20 constructor(protected readonly repository: Repository<BookEntity>) { super(repository); }\n\
                   }\n\
                   export class LooseService extends BookService {\n\
                   \x20 constructor(readonly repository: Repository<BookEntity>) { super(repository); }\n\
                   }\n";
    let book = "src/modules/book/book.service.ts";
    let shelf = "src/modules/shelf/shelf.service.ts";
    let rare = "src/modules/book/rare.service.ts";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[(book, &decorated), (shelf, &based), (rare, derived)]
        ),
        [
            row("", book, Some(5), &message(book, 5, "BookService")),
            row("", rare, Some(6), &message(rare, 6, "LooseService")),
            row("", shelf, Some(3), &message(shelf, 3, "ShelfService")),
        ]
    );
}

#[test]
fn plain_arguments_other_classes_and_other_names_pass() {
    let text = format!(
        "import {{ ApiService }} from \"{AUTOMATOR}\";\n\
         @ApiService({{}})\n\
         export class BookService {{\n\
         \x20 constructor(repository: Repository<BookEntity>, private readonly cache: Cache) {{}}\n\
         }}\n\
         export class Helper {{\n\
         \x20 constructor(private repository: Repository<BookEntity>) {{}}\n\
         }}\n\
         @Other.ApiService({{}})\n\
         export class Unrelated {{\n\
         \x20 constructor(private repository: Repository<BookEntity>) {{}}\n\
         }}\n"
    );

    assert_eq!(
        check(RULE, "{}", &[("src/modules/book/book.service.ts", &text)]),
        []
    );
}

#[test]
fn options_rename_the_package_decorator_base_and_parameter() {
    let text = "import { Service } from \"@scope/crud\";\n\
                @Service()\n\
                export class BookService {\n\
                \x20 constructor(private store: Store) {}\n\
                }\n";
    let path = "src/book.service.ts";

    assert_eq!(
        check(
            RULE,
            r#"{ "framework-package": "@scope/crud", "service-decorator": "Service", "repository-parameter": "store" }"#,
            &[(path, text)]
        ),
        [row(
            "",
            path,
            Some(4),
            &format!(
                "{path}:4 BookService.store must be a protected readonly native repository parameter property"
            )
        )]
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "skip-folders": [] }"#, &[]).contains("skip-folders"));
}

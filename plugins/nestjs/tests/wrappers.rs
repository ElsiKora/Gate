//! `nestjs/wrappers` against synthetic services.

mod host;

use host::{Request, Row, SKIP_DIST, check, error, paths_with, row, rows, run_with, targets};

const RULE: &str = "wrappers";
const SERVICE: &str = "src/modules/book/service.ts";

fn forwards(path: &str, line: u32, name: &str, callee: &str) -> String {
    format!("{path}:{line} {name} only forwards to {callee}; inline the call")
}

fn throw_only(path: &str, line: u32, name: &str) -> String {
    format!(
        "{path}:{line} {name} is a throw-only error wrapper; inline the complete error construction at each decision site"
    )
}

fn raw(path: &str, line: u32, constructor: &str) -> String {
    format!(
        "{path}:{line} {constructor} is constructed without ErrorString; use a direct canonical ErrorString-backed exception"
    )
}

fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort();
    rows
}

/// The aspect, line and target of each finding, sorted.
fn located_targets(
    options: &str,
    exceptions: &[&str],
    files: &[(&str, &str)],
) -> Vec<(String, Option<u32>, Option<String>)> {
    let findings = run_with(
        RULE,
        &Request {
            options,
            exceptions: exceptions
                .iter()
                .map(|target| (*target).to_owned())
                .collect(),
            ..Request::default()
        },
        files,
    )
    .expect("the rule runs");
    let mut found: Vec<_> = rows(&findings)
        .into_iter()
        .zip(targets(&findings))
        .map(|((aspect, _, line, _), target)| (aspect, line, target))
        .collect();
    found.sort();
    found
}

fn at(aspect: &str, line: u32, target: Option<&str>) -> (String, Option<u32>, Option<String>) {
    (aspect.to_owned(), Some(line), target.map(str::to_owned))
}

#[test]
fn each_aspect_is_reported() {
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                export class BookService {\n\
                 constructor(private readonly store: BookStore) {}\n\
                 load(id: string) { return this.store.load(id); }\n\
                 encode(value: unknown): string { return JSON.stringify(value); }\n\
                 freeze(value: object) { return Object.freeze(value); }\n\
                 fail(): never { throw new TypeError(\"failure\"); }\n\
                 reject(invalid: boolean): void { if (invalid) throw new BadRequestException(\"raw\"); }\n\
                }\n";

    assert_eq!(
        sorted(check(RULE, "{}", &[(SERVICE, text)])),
        sorted(vec![
            row(
                "one-call-forwarding",
                SERVICE,
                Some(4),
                &forwards(SERVICE, 4, "load", "this.store.load")
            ),
            row(
                "intrinsic-alias",
                SERVICE,
                Some(5),
                &forwards(SERVICE, 5, "encode", "JSON.stringify")
            ),
            row(
                "intrinsic-alias",
                SERVICE,
                Some(6),
                &forwards(SERVICE, 6, "freeze", "Object.freeze")
            ),
            row(
                "throw-only-error",
                SERVICE,
                Some(7),
                &throw_only(SERVICE, 7, "fail")
            ),
            row(
                "raw-nest-exception",
                SERVICE,
                Some(8),
                &raw(SERVICE, 8, "BadRequestException")
            ),
        ])
    );
}

#[test]
fn boundaries_built_values_guards_and_factory_messages_pass() {
    let service = "import { BadRequestException } from \"@nestjs/common\";\n\
                   import { EErrorStringAction, ErrorString } from \"@elsikora/nestjs-crud-automator\";\n\
                   import { BOOK_MESSAGE } from \"./constant\";\n\
                   export class BookService {\n\
                    private readonly reason = REASON;\n\
                    constructor(private readonly store: BookStore) {}\n\
                    @Step() execute(value: Value) { return this.store.execute(value); }\n\
                    onModuleDestroy() { return this.store.stop(); }\n\
                    revoke(session: Session) { return this.store.revoke(session.id, this.reason); }\n\
                    key(value: { id: string }): string { return JSON.stringify([value.id]); }\n\
                    unreachable(value: never): never { throw new TypeError(\"unreachable\"); }\n\
                    reject(invalid: boolean): void {\n\
                     if (invalid) throw new BadRequestException(ErrorString({ entity: BookEntity, type: EErrorStringAction.INVALID_DATA }));\n\
                     if (!invalid) throw new BadRequestException(BOOK_MESSAGE.INVALID);\n\
                    }\n\
                   }\n";
    let barrel = "export { BOOK_MESSAGE } from \"./message.constant\";\n";
    let constant = "import { EErrorStringAction, ErrorString } from \"@elsikora/nestjs-crud-automator\";\n\
                    export const BOOK_MESSAGE = Object.freeze({ INVALID: ErrorString({ entity: BookEntity, type: EErrorStringAction.INVALID_DATA }) });\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (SERVICE, service),
                ("src/modules/book/constant/index.ts", barrel),
                ("src/modules/book/constant/message.constant.ts", constant),
            ]
        ),
        []
    );
}

#[test]
fn files_outside_the_source_root_declarations_and_broken_files_are_skipped() {
    let wrapper = "export function load(id: string) { return store.load(id); }\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("test/book.ts", wrapper),
                ("src/types/book.d.ts", wrapper),
                (
                    "src/broken.ts",
                    "export function load(id: string) { return store.load(id); }\nclass {"
                ),
            ]
        ),
        []
    );
}

#[test]
fn a_private_constant_failure_guard_is_read_through_its_branch() {
    let guarded = "export class BookService {\n\
                    private check(failed: boolean): void { if (failed) { throw new TypeError(\"x\"); } }\n\
                    run(): void { this.check(true); }\n\
                   }\n";
    let varied = "export class BookService {\n\
                   private check(failed: boolean): void { if (failed) { throw new TypeError(\"x\"); } }\n\
                   run(flag: boolean): void { this.check(true); this.check(flag); }\n\
                  }\n";

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, guarded)]),
        [row(
            "throw-only-error",
            SERVICE,
            Some(2),
            &throw_only(SERVICE, 2, "check")
        )]
    );
    assert_eq!(check(RULE, "{}", &[(SERVICE, varied)]), []);
}

const LIBRARY: &str = "src/modules/library/service.ts";
const LIBRARY_TEXT: &str = "import { ServiceUnavailableException, ServiceUnavailableException as Unavailable } from \"@nestjs/common\";\n\
import * as nest from \"@nestjs/common\";\n\
export class LibraryService {\n\
 open(closed: boolean, late: boolean): void {\n\
  if (closed) throw new ServiceUnavailableException({ status: ELibraryStatus.CLOSED });\n\
  if (late) throw new Unavailable({\n\
   status: ELibraryStatus.CLOSED, // only the status\n\
  });\n\
  try { check(); } catch { throw new nest.ServiceUnavailableException({ status: ELibraryStatus.CLOSED }); }\n\
 }\n\
 closed(): never {\n\
  const error = new ServiceUnavailableException({ status: ELibraryStatus.CLOSED });\n\
  throw error;\n\
 }\n\
}\n";

#[test]
fn a_raw_exception_target_names_its_scope_catch_position_and_canonical_construction() {
    let open = "src/modules/library/service.ts: LibraryService.open: new ServiceUnavailableException({ status: ELibraryStatus.CLOSED })";
    let caught = "src/modules/library/service.ts: LibraryService.open catch: new ServiceUnavailableException({ status: ELibraryStatus.CLOSED })";
    let closed = "src/modules/library/service.ts: LibraryService.closed: new ServiceUnavailableException({ status: ELibraryStatus.CLOSED })";

    assert_eq!(
        located_targets("{}", &[], &[(LIBRARY, LIBRARY_TEXT)]),
        [
            at("raw-nest-exception", 5, Some(open)),
            at("raw-nest-exception", 6, Some(open)),
            at("raw-nest-exception", 9, Some(caught)),
            at("raw-nest-exception", 12, Some(closed)),
            at("throw-only-error", 11, None),
        ]
    );
}

#[test]
fn a_construction_an_exception_names_is_no_throw_only_error_construction() {
    // The exception is spelled differently; the rule compares canonical forms.
    let exception = "src/modules/library/service.ts: LibraryService.closed: new ServiceUnavailableException( { status : ELibraryStatus.CLOSED , } )";
    let found = located_targets("{}", &[exception], &[(LIBRARY, LIBRARY_TEXT)]);

    assert!(
        found
            .iter()
            .all(|(aspect, _, _)| aspect == "raw-nest-exception"),
        "{found:?}"
    );
    assert_eq!(found.len(), 4);
}

#[test]
fn a_construction_passed_first_to_a_call_is_named_with_the_call() {
    let path = "src/common/filter/failure.filter.ts";
    let text = "import { IntrinsicException } from \"@nestjs/common\";\n\
                export class FailureFilter extends BaseExceptionFilter {\n\
                 handleUnknownError(exception: unknown, host: ArgumentsHost): void {\n\
                  super.handleUnknownError(new IntrinsicException(), host);\n\
                  super.handleUnknownError(host, new IntrinsicException());\n\
                 }\n\
                 local(): void {\n\
                  const handle = (error: unknown) => error;\n\
                  handle(new IntrinsicException());\n\
                  relay(new IntrinsicException());\n\
                 }\n\
                }\n";
    let target =
        |scope: &str, construction: &str| format!("{path}: FailureFilter.{scope}: {construction}");
    let call = target(
        "handleUnknownError",
        "super.handleUnknownError(new IntrinsicException())",
    );
    let second = target("handleUnknownError", "new IntrinsicException()");
    let local = target("local", "new IntrinsicException()");
    let relayed = target("local", "relay(new IntrinsicException())");

    assert_eq!(
        located_targets("{}", &[], &[(path, text)]),
        [
            at("raw-nest-exception", 4, Some(&call)),
            at("raw-nest-exception", 5, Some(&second)),
            at("raw-nest-exception", 9, Some(&local)),
            at("raw-nest-exception", 10, Some(&relayed)),
        ]
    );
    // An exception that names only the construction names it in the call too.
    assert_eq!(
        located_targets("{}", &[&local], &[(path, text)])[3],
        at("raw-nest-exception", 10, Some(&local))
    );
}

#[test]
fn callback_parameters_are_named_and_other_local_reads_are_not() {
    let path = "src/runtime/start.function.ts";
    let text = "import { BadRequestException, ValidationPipe } from \"@nestjs/common\";\n\
                export function startLibrary(application: Application): void {\n\
                 application.useGlobalPipes(new ValidationPipe({ exceptionFactory: (errors: Array<BookError>, context: string) => new BadRequestException({ context, errors }) }));\n\
                }\n\
                export class ShelfFactory {\n\
                 create(limit: number): void {\n\
                  check();\n\
                  throw new BadRequestException({ limit });\n\
                 }\n\
                 relay(): void {\n\
                  check();\n\
                  throw new BadRequestException({ value: arguments[0] });\n\
                 }\n\
                }\n\
                class Shelf {}\n\
                function Shelf2() {}\n\
                throw new BadRequestException({ code: \"TOP\" });\n";

    assert_eq!(
        located_targets("{}", &[], &[(path, text)]),
        [
            at(
                "raw-nest-exception",
                3,
                Some(&format!(
                    "{path}: startLibrary parameters errors, context: new BadRequestException({{ context, errors }})"
                ))
            ),
            at(
                "raw-nest-exception",
                8,
                Some(&format!(
                    "{path}: ShelfFactory.create parameters limit: new BadRequestException({{ limit }})"
                ))
            ),
            at(
                "raw-nest-exception",
                12,
                Some(&format!(
                    "{path}:12: new BadRequestException({{ value: arguments[0] }})"
                ))
            ),
            at(
                "raw-nest-exception",
                17,
                Some(&format!(
                    "{path}:17: new BadRequestException({{ code: \"TOP\" }})"
                ))
            ),
        ]
    );
}

#[test]
fn own_parameters_are_named_and_reassigned_or_defaulted_ones_and_locals_are_not() {
    let path = "src/modules/shelf/finder.ts";
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                export class ShelfFinder {\n\
                 findListed(baseUrl: string): void {\n\
                  check();\n\
                  throw new BadRequestException({ baseUrl });\n\
                 }\n\
                 moved(limit: number): void {\n\
                  check();\n\
                  limit = limit + 1;\n\
                  throw new BadRequestException({ limit });\n\
                 }\n\
                 local(): void {\n\
                  check();\n\
                  const shelf = read();\n\
                  throw new BadRequestException({ shelf });\n\
                 }\n\
                 nested(owner: string, items: Array<Item>): void {\n\
                  check();\n\
                  items.forEach((item: Item) => { throw new BadRequestException({ item, owner }); });\n\
                 }\n\
                }\n\
                export function listShelves(page: number, size = 10): void {\n\
                 if (page > 1) throw new BadRequestException({ page });\n\
                 throw new BadRequestException({ size });\n\
                }\n";
    let named = |scope: &str, construction: &str| {
        Some(format!(
            "{path}: {scope}: new BadRequestException({{ {construction} }})"
        ))
    };
    let lined = |line: u32, construction: &str| {
        Some(format!(
            "{path}:{line}: new BadRequestException({{ {construction} }})"
        ))
    };
    let expected = [
        (
            5,
            named("ShelfFinder.findListed parameters baseUrl", "baseUrl"),
        ),
        (10, lined(10, "limit")),
        (15, lined(15, "shelf")),
        (
            19,
            named("ShelfFinder.nested parameters owner, item", "item, owner"),
        ),
        (23, named("listShelves parameters page", "page")),
        (24, lined(24, "size")),
    ];

    assert_eq!(
        located_targets("{}", &[], &[(path, text)]),
        expected
            .iter()
            .map(|(line, target)| at("raw-nest-exception", *line, target.as_deref()))
            .collect::<Vec<_>>()
    );

    // The scope form stays as it is when a line is added above it; the line form moves.
    let moved = text.replacen('\n', "\n// A line added above.\n", 1);
    let found = located_targets("{}", &[], &[(path, &moved)]);
    assert_eq!(
        found[0],
        at(
            "raw-nest-exception",
            6,
            named("ShelfFinder.findListed parameters baseUrl", "baseUrl").as_deref()
        )
    );
    assert_eq!(
        found[1],
        at("raw-nest-exception", 11, lined(11, "limit").as_deref())
    );
}

#[test]
fn rest_and_property_parameters_are_named_and_destructured_or_redeclared_ones_are_not() {
    let path = "src/modules/shelf/store.ts";
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                export class ShelfStore {\n\
                 constructor(private readonly limit: number) {\n\
                  check();\n\
                  throw new BadRequestException({ limit });\n\
                 }\n\
                 find({ id }: Query): void {\n\
                  check();\n\
                  throw new BadRequestException({ id });\n\
                 }\n\
                 redo(size: number): void {\n\
                  check();\n\
                  var size;\n\
                  throw new BadRequestException({ size });\n\
                 }\n\
                 all(...ids: Array<string>): void {\n\
                  check();\n\
                  throw new BadRequestException({ ids });\n\
                 }\n\
                }\n";
    let named = |scope: &str, construction: &str| {
        format!("{path}: ShelfStore.{scope}: new BadRequestException({{ {construction} }})")
    };
    let lined = |line: u32, construction: &str| {
        format!("{path}:{line}: new BadRequestException({{ {construction} }})")
    };

    assert_eq!(
        located_targets("{}", &[], &[(path, text)]),
        [
            at(
                "raw-nest-exception",
                5,
                Some(&named("constructor parameters limit", "limit"))
            ),
            at("raw-nest-exception", 9, Some(&lined(9, "id"))),
            at("raw-nest-exception", 14, Some(&lined(14, "size"))),
            at(
                "raw-nest-exception",
                18,
                Some(&named("all parameters ids", "ids"))
            ),
        ]
    );
}

#[test]
fn an_arrow_field_s_own_parameters_are_named() {
    let path = "src/modules/shelf/guard.ts";
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                export class ShelfGuard {\n\
                 guard = (limit: number): void => {\n\
                  check();\n\
                  throw new BadRequestException({ limit });\n\
                 };\n\
                 run = (owner: string, items: Array<Item>): void => {\n\
                  check();\n\
                  items.forEach((item: Item) => { throw new BadRequestException({ item, owner }); });\n\
                 };\n\
                 accessor verify = (code: string, items: Array<Item>): void => {\n\
                  check();\n\
                  items.forEach((item: Item) => { throw new BadRequestException({ code, item }); });\n\
                 };\n\
                }\n";
    let named = |scope: &str, construction: &str| {
        format!("{path}: ShelfGuard.{scope}: new BadRequestException({{ {construction} }})")
    };

    // The field's arrow is the member's own function, for a property and for an accessor: its
    // parameters are named, and a callback within it adds its own after them. Were the arrow
    // taken for a callback, the reads of `owner` and `code` would keep the line form.
    assert_eq!(
        located_targets("{}", &[], &[(path, text)]),
        [
            at(
                "raw-nest-exception",
                5,
                Some(&named("guard parameters limit", "limit"))
            ),
            at(
                "raw-nest-exception",
                9,
                Some(&named("run parameters owner, item", "item, owner"))
            ),
            at(
                "raw-nest-exception",
                13,
                Some(&named("verify parameters code, item", "code, item"))
            ),
        ]
    );
}

#[test]
fn a_function_scope_named_like_a_class_of_the_file_is_not_named() {
    let path = "src/runtime/shelf.ts";
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                export function Shelf(): void { check(); throw new BadRequestException(\"x\"); }\n\
                namespace inner { export class Shelf {} }\n";

    assert_eq!(
        located_targets("{}", &[], &[(path, text)]),
        [at(
            "raw-nest-exception",
            2,
            Some(&format!("{path}:2: new BadRequestException(\"x\")"))
        )]
    );
}

#[test]
fn plain_errors_are_an_option() {
    let text = "export function fail(): void { throw new BookError(\"x\"); }\n";

    assert_eq!(check(RULE, "{}", &[(SERVICE, text)]), []);
    assert_eq!(
        check(
            RULE,
            r#"{ "plain-errors": ["BookError"] }"#,
            &[(SERVICE, text)]
        ),
        [row(
            "throw-only-error",
            SERVICE,
            Some(1),
            &throw_only(SERVICE, 1, "fail")
        )]
    );
}

#[test]
fn exception_packages_and_suffix_are_options() {
    let text = "import { BookFault } from \"@library/errors\";\n\
                export function fail(flag: boolean): void { if (flag) throw new BookFault(\"x\"); }\n";
    let options = r#"{ "exception-packages": ["@library/errors"], "exception-suffix": "Fault" }"#;

    assert_eq!(check(RULE, "{}", &[(SERVICE, text)]), []);
    assert_eq!(
        check(RULE, options, &[(SERVICE, text)]),
        [row(
            "raw-nest-exception",
            SERVICE,
            Some(2),
            &raw(SERVICE, 2, "BookFault")
        )]
    );
}

#[test]
fn the_message_factory_is_an_option() {
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                import { Message } from \"@library/messages\";\n\
                export function fail(flag: boolean): void {\n\
                 if (flag) throw new BadRequestException(Message(\"x\"));\n\
                 throw new BadRequestException(\"y\");\n\
                }\n";
    let options = r#"{ "message-factory": { "package": "@library/messages", "name": "Message" } }"#;

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, text)]),
        [
            row(
                "raw-nest-exception",
                SERVICE,
                Some(4),
                &raw(SERVICE, 4, "BadRequestException")
            ),
            row(
                "raw-nest-exception",
                SERVICE,
                Some(5),
                &raw(SERVICE, 5, "BadRequestException")
            ),
        ]
    );
    assert_eq!(
        check(RULE, options, &[(SERVICE, text)]),
        [row(
            "raw-nest-exception",
            SERVICE,
            Some(5),
            &format!(
                "{SERVICE}:5 BadRequestException is constructed without Message; use a direct canonical Message-backed exception"
            )
        )]
    );
}

#[test]
fn intrinsic_calls_are_an_option() {
    let text = "export function copy(value: Book) { return Book.copy(value); }\n";

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, text)]),
        [row(
            "one-call-forwarding",
            SERVICE,
            Some(1),
            &forwards(SERVICE, 1, "copy", "Book.copy")
        )]
    );
    assert_eq!(
        check(
            RULE,
            r#"{ "intrinsic-calls": [["Book", "copy"]] }"#,
            &[(SERVICE, text)]
        ),
        [row(
            "intrinsic-alias",
            SERVICE,
            Some(1),
            &forwards(SERVICE, 1, "copy", "Book.copy")
        )]
    );
}

#[test]
fn lifecycle_and_framework_methods_are_options() {
    let lifecycle = "export class BookService { onShelve() { return this.store.stop(); } }\n";
    let handler_path = "src/modules/book/handler/book.handler.ts";
    let handler =
        "export class BookHandler { handle(event: Event) { return this.store.save(event); } }\n";

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, lifecycle)]),
        [row(
            "one-call-forwarding",
            SERVICE,
            Some(1),
            &forwards(SERVICE, 1, "onShelve", "this.store.stop")
        )]
    );
    assert_eq!(
        check(
            RULE,
            r#"{ "lifecycle-methods": ["onShelve"] }"#,
            &[(SERVICE, lifecycle)]
        ),
        []
    );
    assert_eq!(check(RULE, "{}", &[(handler_path, handler)]).len(), 1);
    assert_eq!(
        check(
            RULE,
            r#"{ "framework-methods": { "/handler/": ["handle"] } }"#,
            &[(handler_path, handler)]
        ),
        []
    );
}

#[test]
fn reporter_adapters_are_an_option() {
    let path = "src/infrastructure/report/adapter.ts";
    let text = "import { ReporterPort } from \"@common/report\";\n\
                export class ReportAdapter extends ReporterPort {\n\
                 report(error: Error) { return this.sink.report(error); }\n\
                }\n";
    let options = r#"{ "reporter-adapters": [{ "module": "@common/report", "port": "ReporterPort", "method": "report", "path-prefix": "src/infrastructure/" }] }"#;

    assert_eq!(
        check(RULE, "{}", &[(path, text)]),
        [row(
            "one-call-forwarding",
            path,
            Some(3),
            &forwards(path, 3, "report", "this.sink.report")
        )]
    );
    assert_eq!(check(RULE, options, &[(path, text)]), []);
    // The port is `port`; `class` is no option, and the error names the field to use.
    let refused = error(RULE, &options.replace("\"port\"", "\"class\""), &[]);
    assert!(refused.contains("`class`"), "{refused}");
    assert!(refused.contains("`port`"), "{refused}");
}

#[test]
fn a_reporter_adapter_may_implement_an_interface_port() {
    let path = "src/infrastructure/report/adapter.ts";
    let text = "import type { ReporterPort } from \"@common/report\";\n\
                import { AuditPort } from \"@common/audit\";\n\
                export class ReportAdapter implements ReporterPort {\n\
                 report(error: Error) { return this.sink.report(error); }\n\
                }\n\
                export class AuditAdapter implements AuditPort {\n\
                 report(error: Error) { return this.sink.report(error); }\n\
                }\n";
    let options = r#"{ "reporter-adapters": [{ "module": "@common/report", "port": "ReporterPort", "method": "report", "path-prefix": "src/infrastructure/" }] }"#;

    // The class implementing the port forwards at a boundary; the one implementing another
    // interface does not.
    assert_eq!(
        check(RULE, options, &[(path, text)]),
        [row(
            "one-call-forwarding",
            path,
            Some(7),
            &forwards(path, 7, "report", "this.sink.report")
        )]
    );
}

#[test]
fn a_port_of_the_same_name_from_another_module_is_no_boundary() {
    let options = r#"{ "reporter-adapters": [{ "module": "@common/report", "port": "ReporterPort", "method": "report", "path-prefix": "src/infrastructure/" }] }"#;
    let forwarder = " report(error: Error) { return this.sink.report(error); }\n}\n";
    let implements_text = format!(
        "import {{ ReporterPort }} from \"@common/other\";\nexport class OtherAdapter implements ReporterPort {{\n{forwarder}"
    );
    let extends_text = format!(
        "import {{ ReporterPort }} from \"@common/other\";\nexport class OtherAdapter extends ReporterPort {{\n{forwarder}"
    );
    let implements_path = "src/infrastructure/report/implemented.ts";
    let extends_path = "src/infrastructure/report/extended.ts";

    // The name is the port's, but the module is not the port's, so neither forwarder is a
    // boundary.
    assert_eq!(
        sorted(check(
            RULE,
            options,
            &[
                (implements_path, implements_text.as_str()),
                (extends_path, extends_text.as_str())
            ]
        )),
        sorted(vec![
            row(
                "one-call-forwarding",
                extends_path,
                Some(3),
                &forwards(extends_path, 3, "report", "this.sink.report")
            ),
            row(
                "one-call-forwarding",
                implements_path,
                Some(3),
                &forwards(implements_path, 3, "report", "this.sink.report")
            ),
        ])
    );
}

#[test]
fn a_reporter_adapter_port_is_found_among_several_aliased_generic_or_beside_a_base() {
    let options = r#"{ "reporter-adapters": [{ "module": "@common/report", "port": "ReporterPort", "method": "report", "path-prefix": "src/infrastructure/" }] }"#;
    let forwarder = " report(error: Error) { return this.sink.report(error); }\n}\n";
    let several = format!(
        "import {{ ReporterPort }} from \"@common/report\";\nimport {{ OtherPort }} from \"@common/other\";\nexport class SeveralAdapter implements OtherPort, ReporterPort {{\n{forwarder}"
    );
    let aliased = format!(
        "import {{ ReporterPort as Port }} from \"@common/report\";\nexport class AliasedAdapter implements Port {{\n{forwarder}"
    );
    let generic = format!(
        "import {{ ReporterPort }} from \"@common/report\";\nexport class GenericAdapter<T> implements ReporterPort<T> {{\n{forwarder}"
    );
    let based = format!(
        "import {{ ReporterPort }} from \"@common/report\";\nimport {{ Base }} from \"@common/base\";\nexport class BasedAdapter extends Base implements ReporterPort {{\n{forwarder}"
    );
    let files = [
        ("src/infrastructure/report/several.ts", several.as_str()),
        ("src/infrastructure/report/aliased.ts", aliased.as_str()),
        ("src/infrastructure/report/generic.ts", generic.as_str()),
        ("src/infrastructure/report/based.ts", based.as_str()),
    ];

    // Each class implements the port, so each forwarder is a boundary.
    let found = check(RULE, options, &files);
    assert!(found.is_empty(), "{found:?}");
}

const TABLE: &str = "//! Generated.\n\
pub const PACKAGES: [(&str, &[&str]); 2] = [\n\
    (\"@elsikora/nestjs-crud-automator\", &AUTOMATOR),\n\
    (\"@nestjs/common\", &NEST_COMMON),\n\
];\n\
\n\
const AUTOMATOR: [&str; 1] = [\"ErrorString\"];\n\
\n\
const NEST_COMMON: [&str; 1] = [\n\
    \"BadRequestException\", // the only one\n\
];\n";

#[test]
fn the_package_exports_file_lists_the_known_exports() {
    let text = "import { BadRequestException, NotFoundException } from \"@nestjs/common\";\n\
                export function fail(flag: boolean): void {\n\
                 if (flag) throw new BadRequestException(\"x\");\n\
                 throw new NotFoundException(\"y\");\n\
                }\n";
    let options = r#"{ "package-exports-file": "tools/packages.rs" }"#;

    assert_eq!(check(RULE, "{}", &[(SERVICE, text)]).len(), 2);
    assert_eq!(
        check(
            RULE,
            options,
            &[(SERVICE, text), ("tools/packages.rs", TABLE)]
        ),
        [row(
            "raw-nest-exception",
            SERVICE,
            Some(3),
            &raw(SERVICE, 3, "BadRequestException")
        )]
    );
}

#[test]
fn a_missing_or_malformed_package_exports_file_is_an_error() {
    let options = r#"{ "package-exports-file": "tools/packages.rs" }"#;

    assert!(
        error(RULE, options, &[]).contains("tools/packages.rs is not a tracked text file"),
        "{}",
        error(RULE, options, &[])
    );
    assert!(
        error(
            RULE,
            options,
            &[(
                "tools/packages.rs",
                "pub const PACKAGES: [(&str, &[&str]); 1] = [(\"a\", &MISSING)];\n"
            )]
        )
        .contains("names the list MISSING")
    );
    assert!(
        error(
            RULE,
            options,
            &[("tools/packages.rs", "const OTHER: [&str; 0] = [];\n")]
        )
        .contains("declares no PACKAGES table")
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "package-exports": {} }"#, &[]).contains("package-exports"));
}

#[test]
fn an_empty_message_factory_package_is_an_error() {
    let options = r#"{ "message-factory": { "package": "", "name": "Message" } }"#;

    assert!(error(RULE, options, &[]).contains("`message-factory.package` is empty"));
}

/// A function body inside a construction prints from its source text, which is read without
/// knowing regular expression literals: a `"` or `/*` in one could misprint it. A target is never
/// a misprint: it is the construction exactly, or the `<path>:<line>` target when the printed form
/// is not its own canonical form.
#[test]
fn a_regular_expression_in_a_printed_function_body_is_never_misprinted() {
    let path = "src/library/shelf.service.ts";
    let text = "import { BadRequestException } from \"@nestjs/common\";\n\
                export class ShelfService {\n\
                 quote(): void {\n\
                  check();\n\
                  throw new BadRequestException({ test: () => { return /\"a/.test(\"b\"); } });\n\
                 }\n\
                 comment(): void {\n\
                  check();\n\
                  throw new BadRequestException({ test: () => { return /a\\/*b/.test(\"c\"); } });\n\
                 }\n\
                }\n";
    let found = located_targets("{}", &[], &[(path, text)]);

    let exact = [
        (
            5,
            "ShelfService.quote",
            r#"new BadRequestException({ test: () => { return /"a/.test("b"); } })"#,
        ),
        (
            9,
            "ShelfService.comment",
            r#"new BadRequestException({ test: () => { return /a\/*b/.test("c"); } })"#,
        ),
    ];

    assert_eq!(found.len(), 2);
    for ((aspect, line, target), (expected, method, construction)) in found.iter().zip(exact) {
        let target = target.as_deref().unwrap_or_default();

        assert_eq!(aspect, "raw-nest-exception");
        assert_eq!(*line, Some(expected));
        assert!(
            target == format!("{path}: {method}: {construction}")
                || target.starts_with(&format!("{path}:{expected}: ")),
            "{target:?}"
        );
    }
}

#[test]
fn re_exports_are_followed_when_the_source_root_is_the_project_root() {
    let service = "import { BadRequestException } from \"@nestjs/common\";\n\
                   import { BOOK_MESSAGE } from \"./constant\";\n\
                   export class BookService {\n\
                    reject(): void { check(); throw new BadRequestException(BOOK_MESSAGE.INVALID); }\n\
                   }\n";
    let barrel = "export { BOOK_MESSAGE } from \"./message.constant\";\n";
    let constant = "import { EErrorStringAction, ErrorString } from \"@elsikora/nestjs-crud-automator\";\n\
                    export const BOOK_MESSAGE = Object.freeze({ INVALID: ErrorString({ entity: BookEntity, type: EErrorStringAction.INVALID_DATA }) });\n";
    let findings = run_with(
        RULE,
        &Request {
            plugin_options: r#"{ "layout": { "source-root": "" } }"#,
            ..Request::default()
        },
        &[
            ("modules/book/service.ts", service),
            ("modules/book/constant/index.ts", barrel),
            ("modules/book/constant/message.constant.ts", constant),
        ],
    )
    .expect("the rule runs");

    assert_eq!(rows(&findings), []);
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let wrapper = "export function load(id: string) { return store.load(id); }\n";
    let files = [
        ("src/dist/book.ts", wrapper),
        ("src/generated/book.ts", wrapper),
    ];

    assert_eq!(
        paths_with(RULE, "{}", SKIP_DIST, &files),
        ["src/dist/book.ts", "src/generated/book.ts"]
    );
}

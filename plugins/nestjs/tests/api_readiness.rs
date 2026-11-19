//! `nestjs/api-readiness` against synthetic startup files.

mod host;

use host::{check, error, row, run};

const RULE: &str = "api-readiness";
const START: &str = "src/start.ts";
const READINESS: &str = "src/readiness.ts";
const OPTIONS: &str = r#"{
    "readiness-files": ["src/readiness.ts"],
    "startup-file": "src/start.ts",
    "entrypoint": "libraryStart",
    "application": "library",
    "readiness-provider": "LibraryReadiness",
    "lifecycle": "shelf",
    "calls": { "get": "get", "assert-ready": "check", "listen": "open", "mark-ready": "markOpen" }
}"#;
const ORDER: &str = "src/start.ts libraryStart must await library.get(LibraryReadiness).check, then await library.open, then call shelf.markOpen in that order in the entrypoint or one locally bound, once-awaited startup operation";

/// Each case: its name, the line of the startup-order finding (none for a passing case) and the
/// startup file.
const CASES: &[(&str, Option<u32>, &str)] = &[
    // A throw that a catch clause takes leaves the try statement when the catch clause returns.
    (
        "invalid-exit-catch",
        Some(1),
        r#"export async function libraryStart(): Promise<void> {
 try {
  throw new Error("stop");
 } catch {
  return;
 }
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
"#,
    ),
    // A return in a finally block leaves the try statement whatever its try block does.
    (
        "invalid-exit-finally",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 try {
  prepare();
 } finally {
  return;
 }
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
",
    ),
    // Lifecycle calls after an unconditional throw never run.
    (
        "invalid-exit-throw",
        Some(1),
        r#"export async function libraryStart(): Promise<void> {
 throw new Error("stop");
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
"#,
    ),
    // A return in a try block leaves it even when a catch clause exists, because only a throw reaches the catch.
    (
        "invalid-exit-try-return",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 try {
  return;
 } catch {
  // Never reached by the return.
 }
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
",
    ),
    // The listener must be library.open.
    (
        "invalid-listener-receiver",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await library.get(LibraryReadiness).check();
 await archive.open(3000);
 shelf.markOpen();
}
",
    ),
    // The ready mark takes no argument.
    (
        "invalid-mark-ready-argument",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen(true);
}
",
    ),
    // An awaited ready mark is not the plain call the order requires.
    (
        "invalid-mark-ready-awaited",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 await shelf.markOpen();
}
",
    ),
    // The ready mark must be shelf.markOpen.
    (
        "invalid-mark-ready-receiver",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 archive.markOpen();
}
",
    ),
    // A startup operation awaited with an argument is no execution scope.
    (
        "invalid-operation-argument",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 await startup(3000);
}
",
    ),
    // A startup operation also referenced from a class field is referenced twice, even when the class has a heritage clause.
    (
        "invalid-operation-class-field",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 class Later extends Object {
  field = startup;
 }
 await startup();
}
",
    ),
    // A startup operation bound with let is no execution scope.
    (
        "invalid-operation-let",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 let startup = async (): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 await startup();
}
",
    ),
    // A startup operation with a parameter is no execution scope.
    (
        "invalid-operation-parameter",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (port = 3000): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 await startup();
}
",
    ),
    // A startup operation that is only passed on, never called, is no execution scope.
    (
        "invalid-operation-reference",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 await run(startup);
}
",
    ),
    // A rejected startup operation fails the entrypoint even when the entrypoint itself holds the three calls.
    (
        "invalid-operation-rejected-extra",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const extra = (): void => {
  shelf.markOpen();
 };
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 await extra();
}
",
    ),
    // A startup operation with a rest parameter is no execution scope.
    (
        "invalid-operation-rest",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (...ports: number[]): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 await startup();
}
",
    ),
    // A startup operation that is not async is no execution scope.
    (
        "invalid-operation-sync",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = (): Promise<void> => {
  return library
   .get(LibraryReadiness)
   .check()
   .then(() => library.open(3000));
 };
 await startup();
 shelf.markOpen();
}
",
    ),
    // A startup operation awaited twice is no execution scope.
    (
        "invalid-operation-twice",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 await startup();
 await startup();
}
",
    ),
    // A startup operation called without await is no execution scope.
    (
        "invalid-operation-unawaited",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 void startup();
}
",
    ),
    // A lifecycle call in the computed key of a plain object property runs in the entrypoint and counts.
    (
        "invalid-property-key-call",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 const keyed = { [String(shelf.markOpen())]: 1 };
}
",
    ),
    // The readiness check must be resolved for LibraryReadiness alone.
    (
        "invalid-readiness-provider",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await library.get(LibraryReadiness, { strict: false }).check();
 await library.open(3000);
 shelf.markOpen();
}
",
    ),
    // The readiness check must come from library.
    (
        "invalid-readiness-receiver",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 await archive.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
",
    ),
    // A startup operation awaited only on one branch is no execution scope; its lifecycle calls then count but form no group.
    (
        "invalid-startup-operation",
        Some(1),
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
  await library.get(LibraryReadiness).check(shelf);
  await library.open({ port: 3000 });
  shelf.markOpen();
 };

 if (process.env.START) {
  await startup();
 }
}
",
    ),
    // libraryStart marks the role ready before it opens; the finding is at the export, after the JSDoc.
    (
        "invalid-startup-order",
        Some(6),
        r#"import { LibraryReadiness } from "./readiness";

/**
 * Opens the library.
 */
export
async function libraryStart(): Promise<void> {
 const library = await createLibrary();
 await library.get(LibraryReadiness).check(shelf);
 shelf.markOpen();
 await library.open({ port: 3000 });
}
"#,
    ),
    // Lifecycle calls in nested functions, class members, object methods and accessors with computed names, a helper arrow without lifecycle calls, a listener inside the readiness call, a caught throw, a finally block and a break of its own label do not count, and the first libraryStart decides.
    (
        "valid-decoys",
        None,
        r#"export async function libraryStart(): Promise<void> {
 const helper = function (): void {
  shelf.markOpen();
 };
 const format = (value: number): string => {
  return String(value);
 };
 const label = format(3);
 const methods = {
  [String(shelf.markOpen())](): void {},
  get [String(library.open(9))](): number {
   return 1;
  },
 };
 class Decoy {
  listening = library.open(2);

  start(): Promise<void> {
   return library.open(1);
  }
 }

 try {
  throw new Error("retry");
 } catch {
  // Recovered.
 }

 try {
  prepare();
 } finally {
  release();
 }

 outer: {
  break outer;
 }

 {
  await library.get(LibraryReadiness).check(library.open(0));
  await library.open(3000);
  shelf.markOpen();
 }
}

export function libraryStart(): void {
 shelf.markOpen();
}
"#,
    ),
    // A break of an outer label leaves the inner labeled block and stops at its own label, so the statements after the labeled block still run.
    (
        "valid-labeled-break",
        None,
        r"export async function libraryStart(): Promise<void> {
 outer: {
  inner: {
   break outer;
  }
  return;
 }
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
",
    ),
    // A typeof reference to the startup operation in a local type alias is a type position, so the awaited call stays its only reference.
    (
        "valid-operation-type-reference",
        None,
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
  await library.get(LibraryReadiness).check();
  await library.open(3000);
  shelf.markOpen();
 };
 type Operation = typeof startup;
 const typed: Operation | undefined = undefined;
 await startup();
}
",
    ),
    // TypeScript keeps an optional call such as shelf?.markOpen() a call, so optional chains count as lifecycle calls and as the awaited startup operation.
    (
        "valid-optional-chain",
        None,
        r"export async function libraryStart(): Promise<void> {
 const startup = async (): Promise<void> => {
  await library?.get(LibraryReadiness).check();
  await library.open?.(3000);
  shelf?.markOpen();
 };

 await startup?.();
}
",
    ),
    // A startup operation named in a class heritage or an instantiation expression, which the compiler parses as type nodes, is still referenced once.
    (
        "valid-type-positions",
        None,
        r"export async function libraryStart(): Promise<void> {
 const startup = async <TPort,>(): Promise<void> => {
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
 };
 class Later extends (startup as unknown as new () => object) {}
 const typed = startup<number>;
 await startup();
}
",
    ),
    // A startup file oxc cannot parse is skipped, as typescript/parse reports it, instead of stopping the run or being read as a file without startup calls.
    (
        "valid-unparseable-start",
        None,
        r"export async function libraryStart(): Promise<void> {
 const = ;
}
",
    ),
    // A parenthesized or asserted arrow with a lifecycle call is no startup operation, so a reference that does not await it leaves the entrypoint's own ordered calls valid.
    (
        "valid-wrapped-operation",
        None,
        r"export async function libraryStart(): Promise<void> {
 const parenthesized = (async (): Promise<void> => {
  shelf.markOpen();
 });
 const asserted = (async (): Promise<void> => {
  shelf.markOpen();
 }) as () => Promise<void>;
 const satisfying = (async (): Promise<void> => {
  shelf.markOpen();
 }) satisfies () => Promise<void>;
 void parenthesized;
 void asserted;
 void satisfying;
 await library.get(LibraryReadiness).check();
 await library.open(3000);
 shelf.markOpen();
}
",
    ),
    // The readiness owner is tracked and libraryStart awaits check, then open, then marks the role ready in one local startup operation awaited once inside an unconditional try block.
    (
        "valid",
        None,
        r#"import { LibraryReadiness } from "./readiness";

/**
 * Opens the library.
 */
export async function libraryStart(): Promise<void> {
 const library = await createLibrary();
 const startup = async (): Promise<void> => {
  await library.get(LibraryReadiness).check(shelf);
  await library.open({ port: 3000 });
  shelf.markOpen();
 };

 try {
  await startup();
 } catch (error) {
  throw error;
 }
}
"#,
    ),
];

#[test]
fn every_startup_case_reports_exactly_its_expected_finding() {
    for (name, line, text) in CASES {
        let expected: Vec<host::Row> = line
            .iter()
            .map(|line| row("startup-order", START, Some(*line), ORDER))
            .collect();

        assert_eq!(
            check(
                RULE,
                OPTIONS,
                &[
                    (READINESS, "export class LibraryReadiness {}\n"),
                    (START, text)
                ]
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn a_missing_owner_file_startup_file_or_entrypoint_body_is_reported() {
    assert_eq!(
        check(RULE, OPTIONS, &[("src/main.ts", "export {};\n")]),
        [
            row(
                "readiness-owner",
                READINESS,
                None,
                "src/readiness.ts is required for the Runtime API readiness owner"
            ),
            row(
                "startup-order",
                START,
                None,
                "src/start.ts is required for Runtime API lifecycle governance"
            ),
        ]
    );
    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (READINESS, ""),
                (
                    START,
                    "export declare function libraryStart(): Promise<void>;\n"
                )
            ]
        ),
        [row(
            "startup-order",
            START,
            None,
            "src/start.ts must declare libraryStart with an executable body"
        )]
    );
}

#[test]
fn the_defaults_name_the_conventional_files_and_calls() {
    let start = "src/runtime/api/function/start.function.ts";
    let text = "export async function runtimeApiStart(): Promise<void> {\n await roleApplication.listen(3000);\n await roleApplication.get(RuntimeApiReadinessService).assertReady();\n roleLifecycle.markReady();\n}\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("src/runtime/api/readiness/index.ts", ""),
                ("src/runtime/api/readiness/service.ts", ""),
                (start, text),
            ]
        ),
        [
            row(
                "readiness-owner",
                "src/runtime/api/readiness/module.ts",
                None,
                "src/runtime/api/readiness/module.ts is required for the Runtime API readiness owner"
            ),
            row(
                "startup-order",
                start,
                Some(1),
                "src/runtime/api/function/start.function.ts runtimeApiStart must await roleApplication.get(RuntimeApiReadinessService).assertReady, then await roleApplication.listen, then call roleLifecycle.markReady in that order in the entrypoint or one locally bound, once-awaited startup operation"
            ),
        ]
    );
}

#[test]
fn unknown_options_are_errors() {
    assert!(error(RULE, r#"{ "listener": "open" }"#, &[]).contains("listener"));
    assert!(error(RULE, r#"{ "calls": { "ready": "x" } }"#, &[]).contains("ready"));
}

#[test]
fn a_startup_file_that_is_no_source_file_is_a_configuration_error() {
    let options = OPTIONS.replace("\"src/start.ts\"", "\"src/start.json\"");
    let refused = run(RULE, &options, "{}", &[("src/start.json", "{}\n")]).expect_err("refused");
    let message = refused.message();
    assert!(
        message.contains("option `startup-file` is \"src/start.json\"")
            && message.contains("name a file ending in .ts, .tsx"),
        "{message}"
    );
    assert!(refused.is_config(), "a configuration error: {message}");
}

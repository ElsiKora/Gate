//! Static values read through the TypeScript layer, without executing code.

use gate_typescript::value::{StaticValue, ValueReader, number_text};
use gate_typescript::{Allocator, MemorySource, Settings, TypeScript};

/// The value of `FORMAT` followed through `properties` in `src/constant.ts`.
fn exported(source: &str, properties: &[&str]) -> Option<StaticValue> {
    let source = MemorySource::new([("src/constant.ts", source)]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let file = typescript
        .parse("src/constant.ts")
        .expect("parses")
        .expect("tracked");
    let properties: Vec<String> = properties.iter().map(|&item| item.to_owned()).collect();

    ValueReader::new(&typescript)
        .read_export(&file, "FORMAT", &properties)
        .expect("reads")
}

#[test]
fn exported_constant_properties_resolve_through_local_bindings() {
    let number = |value: f64| Some(StaticValue::Number(value));
    let cases = [
        (
            "const privateLength = 64; export const FORMAT = { LENGTH: privateLength };",
            number(64.0),
        ),
        (
            "const length = 64; const fields = { LENGTH: length } as const; const local = fields; export { local as FORMAT };",
            number(64.0),
        ),
        (
            "const oldLength = 64; export const FORMAT = { LENGTH: 63 };",
            number(63.0),
        ),
        (
            "const LENGTH = 64; export const FORMAT = { LENGTH };",
            number(64.0),
        ),
        (
            "const owner = { size: 64 }; export const FORMAT = { LENGTH: owner[\"size\"] };",
            number(64.0),
        ),
        (
            "export const FORMAT = { LENGTH: 64, LENGTH: 63 };",
            number(63.0),
        ),
        ("const FORMAT = { LENGTH: 64 };", None),
        ("export const FORMAT = { LENGTH: computeLength() };", None),
        (
            "const length = alias; const alias = length; export const FORMAT = { LENGTH: length };",
            None,
        ),
        ("export const FORMAT = { LENGTH: 64, ...other };", None),
        (
            "let length = 64; export const FORMAT = { LENGTH: length };",
            None,
        ),
    ];

    for (source, expected) in cases {
        assert_eq!(exported(source, &["LENGTH"]), expected, "{source}");
    }
}

#[test]
fn only_the_built_in_string_raw_tag_yields_raw_text() {
    assert_eq!(
        exported(
            "const pattern = String.raw`/^\\d{6}$/`; export const FORMAT = { CODE: { PATTERN: pattern } };",
            &["CODE", "PATTERN"],
        ),
        Some(StaticValue::String("/^\\d{6}$/".to_owned()))
    );

    for declaration in [
        "const String = customTag;",
        "import { String } from 'custom-tag';",
        "function String() {}",
    ] {
        let source =
            format!("{declaration} export const FORMAT = {{ PATTERN: String.raw`/anything/` }};");
        assert_eq!(exported(&source, &["PATTERN"]), None, "{declaration}");
    }
}

#[test]
fn imports_re_exports_and_namespaces_resolve_across_tracked_files() {
    let source = MemorySource::new([
        (
            "tsconfig.json",
            r#"{ "compilerOptions": { "paths": { "@shared/*": ["./src/shared/*"] } } }"#,
        ),
        (
            "src/shared/value.ts",
            "const prefix = \"current \"; export const VALUE = { DESCRIPTION: prefix + \"state\" };",
        ),
        (
            "src/shared/index.ts",
            "export { VALUE as SCHEMA } from \"./value\";\nexport * from \"./star\";",
        ),
        ("src/shared/star.ts", "export const STAR = \"star\";"),
        (
            "src/shared/type.ts",
            "export type { VALUE } from \"./value\";",
        ),
        (
            "src/use.ts",
            "import { SCHEMA } from \"@shared/index\";\nimport * as Shared from \"./shared\";\nimport { STAR } from \"./shared\";\nimport type { VALUE } from \"./shared/value\";\nexport const FORMAT = { A: SCHEMA.DESCRIPTION, B: Shared.SCHEMA.DESCRIPTION, C: STAR + \"!\", D: VALUE.DESCRIPTION, E: Shared.MISSING };",
        ),
    ]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let reader = ValueReader::new(&typescript);
    let file = typescript
        .parse("src/use.ts")
        .expect("parses")
        .expect("tracked");
    let read = |property: &str| {
        reader
            .read_export(&file, "FORMAT", &[property.to_owned()])
            .expect("reads")
    };
    let string = |value: &str| Some(StaticValue::String(value.to_owned()));

    assert_eq!(read("A"), string("current state"));
    assert_eq!(read("B"), string("current state"));
    assert_eq!(read("C"), string("star!"));
    // A type-only import names no value; a missing export is unknown.
    assert_eq!(read("D"), None);
    assert_eq!(read("E"), None);
}

#[test]
fn numbers_print_as_javascript_prints_them() {
    // Each pair was printed by `String(value)` in Node 24.
    let cases = [
        (16.0, "16"),
        (1e21, "1e+21"),
        (1e-7, "1e-7"),
        (123_456_789_012_345_678_901_234.0, "1.2345678901234569e+23"),
        (0.000_001, "0.000001"),
        (1.5, "1.5"),
        (1e20, "100000000000000000000"),
        (0.1, "0.1"),
        (-2.0, "-2"),
        (5e-324, "5e-324"),
        (0.000_001_234_5, "0.0000012345"),
        (1.797_693_134_862_315_7e308, "1.7976931348623157e+308"),
        (123.456e-10, "1.23456e-8"),
        (0.0, "0"),
        // Exactly halfway between two shortest candidates: the even last digit wins.
        // Written as sums: Clippy reads an exact tie literal as excessive precision.
        (1_798_370_315_440_760.0 + 0.25, "1798370315440760.2"),
        (218_129_524_729_468.0 + 0.125, "218129524729468.12"),
        // A tie whose even candidate does not read back keeps the other one.
        (2f64.powi(-24), "5.960464477539063e-8"),
        (2f64.powi(-25), "2.9802322387695312e-8"),
    ];

    for (value, text) in cases {
        assert_eq!(number_text(value), text, "{value:e}");
    }
}

//! Package bindings read through the TypeScript layer, as the JavaScript native import resolver
//! followed them.

use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::ast::Statement;
use gate_typescript::{Allocator, MemorySource, Settings, TypeScript};

const SOURCE: &str = r#"
import { ApiPropertyDescribe as Describe } from "pkg";
import * as Crud from "pkg";
import type { Typed } from "pkg";
import { createRequire } from "node:module";
import moduleDefault from "node:module";
const { ApiPropertyDescribe: Destructured } = Crud;
const Alias = Crud.ApiPropertyDescribe;
const Chained = Crud?.ApiPropertyDescribe;
const required = require("pkg");
const loader = createRequire(import.meta.url);
const created = loader("pkg");
const dynamic = await import("pkg");
const chainedFactory = createRequire?.(import.meta.url);
const chainedRequired = chainedFactory?.("pkg");
const optional = await import("pkg", { with: { type: "json" } });
const viaDefault = moduleDefault.createRequire(import.meta.url)("pkg");
let mutable = Crud;
Describe;
Crud["Nested"].Member;
Typed;
Destructured;
Alias;
required.Value;
created.Value;
mutable.Value;
Unbound;
Chained;
Crud?.["Nested"]?.Member;
dynamic.Value;
chainedRequired.Value;
optional.Value;
required!.Value;
viaDefault.Value;
"#;

#[test]
fn package_bindings_follow_imports_aliases_destructuring_require_import_and_chains() {
    let source = MemorySource::new([("src/use.ts", SOURCE)]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let file = typescript
        .parse("src/use.ts")
        .expect("parses")
        .expect("tracked");
    let bindings = ImportBindings::new(&file, "pkg", false).expect("the file names pkg");
    let resolved: Vec<Option<String>> = file
        .program()
        .body
        .iter()
        .filter_map(|statement| match statement {
            Statement::ExpressionStatement(statement) => Some(
                bindings
                    .resolve(&statement.expression)
                    .map(|path| path.join(".")),
            ),
            _ => None,
        })
        .collect();

    assert_eq!(
        resolved,
        [
            Some("ApiPropertyDescribe".to_owned()),
            Some("Nested.Member".to_owned()),
            None,
            Some("ApiPropertyDescribe".to_owned()),
            Some("ApiPropertyDescribe".to_owned()),
            Some("Value".to_owned()),
            Some("Value".to_owned()),
            None,
            None,
            Some("ApiPropertyDescribe".to_owned()),
            Some("Nested.Member".to_owned()),
            Some("Value".to_owned()),
            Some("Value".to_owned()),
            None,
            Some("Value".to_owned()),
            Some("Value".to_owned()),
        ]
    );
    assert!(ImportBindings::new(&file, "other", false).is_none());
}

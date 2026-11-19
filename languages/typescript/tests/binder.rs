//! Name lookup within one file as the TypeScript binder performs it: merged, dotted and ambient
//! namespace blocks, `import X = A` aliases, the meaning of each name and `declare global`.

use gate_typescript::binder::{Binder, Meaning};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{TSType, TSTypeName, TSTypeQueryExprName};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::{Allocator, MemorySource, Settings, TypeScript};

const SOURCE: &str = r#"export {};
namespace A {
 export enum E { X = "X" }
 enum Hidden { X = "X" }
 namespace Local { export enum E { X = "X" } }
 export enum F { X = "X" }
}
namespace A {
 type RBlock = E;
 type RHidden = Hidden;
 type RLocal = Local.E;
 export enum F { Y = "Y" }
}
type RMerged = A.F;
namespace B.C { export enum E { X = "X" } }
type RDotted = B.C.E;
namespace S { export enum E { X = "X" } }
namespace T { import X = S; type RAlias = X.E; import Y = Z; import Z = Y; type RCycle = Y.E; }
declare namespace D { enum E { X = "X" } }
type RAmbient = D.E;
type RTypeof = typeof S.E;
namespace W { const S = 1; type RShadowed = typeof S.E; type RNamespace = S.E; }
namespace M { export const Pick = 1; type RMeaning = Pick<{ a: 1 }, "a">; }
declare global { enum G { X = "X" } }
type RGlobal = G;
"#;

/// Each type alias of `source` with the lines of the declarations its name resolves to: a type
/// reference in [`Meaning::Type`] and a `typeof` query in [`Meaning::Value`].
fn resolve(source: &str) -> Vec<(String, Vec<u32>)> {
    let source = MemorySource::new([("src/names.ts", source)]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let file = typescript
        .parse("src/names.ts")
        .expect("parses")
        .expect("tracked");
    let binder = Binder::new(&file);
    let nodes = file.semantic().nodes();
    nodes
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::TSTypeAliasDeclaration(alias) => Some(alias),
            _ => None,
        })
        .map(|alias| {
            let symbol = match &alias.type_annotation {
                TSType::TSTypeReference(reference) => match &reference.type_name {
                    TSTypeName::IdentifierReference(name) => binder.resolve(name, Meaning::Type),
                    TSTypeName::QualifiedName(name) => {
                        binder.resolve_qualified(name, Meaning::Type)
                    }
                    TSTypeName::ThisExpression(_) => None,
                },
                TSType::TSTypeQuery(query) => match &query.expr_name {
                    TSTypeQueryExprName::IdentifierReference(name) => {
                        binder.resolve(name, Meaning::Value)
                    }
                    TSTypeQueryExprName::QualifiedName(name) => {
                        binder.resolve_qualified(name, Meaning::Value)
                    }
                    _ => None,
                },
                _ => None,
            };
            let lines = symbol
                .map(|symbol| {
                    binder
                        .declarations(symbol)
                        .into_iter()
                        .map(|declaration| file.line(nodes.kind(declaration).span().start))
                        .collect()
                })
                .unwrap_or_default();

            (alias.id.name.to_string(), lines)
        })
        .collect()
}

#[test]
fn names_resolve_through_merged_namespaces_aliases_meanings_and_global_blocks() {
    let resolved = resolve(SOURCE);

    assert_eq!(
        resolved,
        [
            ("RBlock".to_owned(), vec![3]),
            ("RHidden".to_owned(), vec![]),
            ("RLocal".to_owned(), vec![]),
            ("RMerged".to_owned(), vec![6, 12]),
            ("RDotted".to_owned(), vec![15]),
            ("RAlias".to_owned(), vec![17]),
            ("RCycle".to_owned(), vec![]),
            ("RAmbient".to_owned(), vec![19]),
            ("RTypeof".to_owned(), vec![17]),
            ("RShadowed".to_owned(), vec![]),
            ("RNamespace".to_owned(), vec![17]),
            ("RMeaning".to_owned(), vec![]),
            ("RGlobal".to_owned(), vec![24]),
        ]
    );
}

#[test]
fn edge_forms_resolve_as_the_typescript_compiler_resolves_them() {
    let module = r#"export {};
enum E { X = "X" }
namespace A { export enum E { X = "X" } }
namespace A { type RShadow = E; }
namespace S { export enum K { X = "X" } }
declare namespace D { import X = S; }
declare namespace D { type RAliasLocal = X.K; }
namespace F { export enum V { X = "X" } }
namespace F { enum V { Y = "Y" } }
type RLocalSameName = F.V;
declare global { namespace G1 { export enum H { X = "X" } } }
declare global { namespace G1 { type RGlobalNamespace = H; } }
"#;
    let script = r#"declare module "m" { export enum M { X = "X" } }
declare module "m" { type RModule = M; }
declare module "n" { export {}; enum N { X = "X" } }
declare module "n" { type RExplicit = N; }
declare module "p" { export * from "q"; enum P { X = "X" } }
declare module "p" { type RExportAll = P; }
declare module "r" { export { V } from "q"; enum W { X = "X" } }
declare module "r" { type RExportFrom = W; }
declare module "q" { export const V = 1; }
declare module "s" { enum T { X = "X" } }
declare module "s" { type RImplicit = T; }
declare module "u" { enum U { X = "X" } const z: U; export = z; }
declare module "u" { type RAssigned = U; }
"#;
    let expected = |cases: &[(&str, &[u32])]| -> Vec<(String, Vec<u32>)> {
        cases
            .iter()
            .map(|(name, lines)| ((*name).to_owned(), lines.to_vec()))
            .collect()
    };

    assert_eq!(
        resolve(module),
        expected(&[
            ("RShadow", &[3]),
            ("RAliasLocal", &[]),
            ("RLocalSameName", &[8]),
            ("RGlobalNamespace", &[11]),
        ])
    );
    assert_eq!(
        resolve(script),
        expected(&[
            ("RModule", &[1]),
            ("RExplicit", &[]),
            ("RExportAll", &[]),
            ("RExportFrom", &[]),
            ("RImplicit", &[10]),
            ("RAssigned", &[]),
        ])
    );
}

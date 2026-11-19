//! The source enum names: no enum below the source root takes the name of an installed
//! `ErrorString` action enum, which would shadow it with a local copy (`error-string-shim`).

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;

use crate::shared::report::Findings;

/// Reports each enum of the file named in `names`.
pub(super) fn report(file: &SourceFile<'_>, path: &str, names: &[String], findings: &mut Findings) {
    for node in file.semantic().nodes().iter() {
        let AstKind::TSEnumDeclaration(declaration) = node.kind() else {
            continue;
        };
        let name = declaration.id.name.as_str();

        if names.iter().any(|known| known == name) {
            findings.push(
                "error-string-shim",
                path,
                Some(file.line(declaration.id.span.start)),
                format!(
                    "{path} declares {name}, a local copy of the installed Crud Automator enum; use its installed members and record a missing action as a follow-up"
                ),
            );
        }
    }
}

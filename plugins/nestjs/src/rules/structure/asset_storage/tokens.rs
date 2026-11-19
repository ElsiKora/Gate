//! What one source file names: identifiers, string literals, loaded modules, member accesses and
//! literal property assignments, collected in one walk of its syntax tree.

use std::collections::BTreeSet;

use gate_typescript::oxc_ast::ast::{
    Argument, BindingIdentifier, CallExpression, ExportAllDeclaration, ExportFromDeclaration,
    Expression, IdentifierName, IdentifierReference, ImportDeclaration, ImportExpression,
    LabelIdentifier, MethodDefinition, MethodDefinitionKind, ObjectProperty, PropertyKey,
    PropertyKind, StaticMemberExpression, StringLiteral, TSClassImplements,
    TSImportEqualsDeclaration, TSIndexSignatureName, TSInterfaceHeritage, TSModuleReference,
    TSTypeName, TemplateLiteral,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::oxc_syntax::scope::ScopeFlags;

/// One `key: "literal"` object property: its key, the string it holds after type assertions and
/// parentheses, and the offset of the property.
#[derive(Debug)]
pub struct Assignment {
    pub key: String,
    pub value: String,
    pub offset: u32,
}

/// The names one file uses, as the TypeScript compiler's node kinds group them.
#[derive(Debug, Default)]
pub struct Tokens {
    /// Every identifier: references, bindings, property and label names.
    pub identifiers: Vec<String>,
    /// Every string literal and template literal without substitutions.
    pub strings: Vec<String>,
    /// Every module specifier an import, export, `import = require`, `require()` or `import()`
    /// loads.
    pub modules: BTreeSet<String>,
    /// `object.property` accesses whose object is an identifier.
    pub members: BTreeSet<(String, String)>,
    pub assignments: Vec<Assignment>,
}

impl Tokens {
    /// Whether an identifier or string literal satisfies `predicate`.
    pub fn any_name(&self, predicate: impl Fn(&str) -> bool) -> bool {
        self.identifiers
            .iter()
            .chain(&self.strings)
            .any(|name| predicate(name))
    }
}

impl<'a> Visit<'a> for Tokens {
    fn visit_identifier_name(&mut self, it: &IdentifierName<'a>) {
        self.identifiers.push(it.name.to_string());
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.identifiers.push(it.name.to_string());
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.identifiers.push(it.name.to_string());
    }

    fn visit_label_identifier(&mut self, it: &LabelIdentifier<'a>) {
        self.identifiers.push(it.name.to_string());
    }

    /// A constructor has no name node for the compiler, so its `constructor` key is skipped.
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        if it.kind == MethodDefinitionKind::Constructor {
            self.visit_decorators(&it.decorators);
            self.visit_function(&it.value, ScopeFlags::Function | ScopeFlags::Constructor);
        } else {
            walk::walk_method_definition(self, it);
        }
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        self.strings.push(it.value.to_string());
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        if let Some(text) = plain_template(it) {
            self.strings.push(text);
        }

        walk::walk_template_literal(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.modules.insert(it.source.value.to_string());
        walk::walk_import_declaration(self, it);
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        self.modules.insert(it.source.value.to_string());
        walk::walk_export_from_declaration(self, it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.modules.insert(it.source.value.to_string());
        walk::walk_export_all_declaration(self, it);
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        if let TSModuleReference::ExternalModuleReference(reference) = &it.module_reference {
            self.modules.insert(reference.expression.value.to_string());
        }

        walk::walk_ts_import_equals_declaration(self, it);
    }

    /// `import.defer()` and `import.source()` load no module for the compiler.
    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if it.phase.is_none()
            && let Some(name) = module_name(&it.source)
        {
            self.modules.insert(name);
        }

        walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = &it.callee
            && callee.name == "require"
            && let Some(name) = it
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .and_then(module_name)
        {
            self.modules.insert(name);
        }

        walk::walk_call_expression(self, it);
    }

    fn visit_ts_index_signature_name(&mut self, it: &TSIndexSignatureName<'a>) {
        self.identifiers.push(it.name.to_string());
        walk::walk_ts_index_signature_name(self, it);
    }

    /// The compiler reads `implements A.B` and `extends A.B` of an interface as property accesses.
    fn visit_ts_class_implements(&mut self, it: &TSClassImplements<'a>) {
        self.heritage(&it.expression);
        walk::walk_ts_class_implements(self, it);
    }

    fn visit_ts_interface_heritage(&mut self, it: &TSInterfaceHeritage<'a>) {
        self.heritage(&it.type_name);
        walk::walk_ts_interface_heritage(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.members
                .insert((object.name.to_string(), it.property.name.to_string()));
        }

        walk::walk_static_member_expression(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.kind == PropertyKind::Init && !it.method && !it.shorthand && !it.computed {
            let key = match &it.key {
                PropertyKey::StaticIdentifier(name) => Some(name.name.to_string()),
                PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
                _ => None,
            };
            let mut value = &it.value;

            loop {
                value = match value {
                    Expression::TSAsExpression(inner) => &inner.expression,
                    Expression::ParenthesizedExpression(inner) => &inner.expression,
                    Expression::TSSatisfiesExpression(inner) => &inner.expression,
                    Expression::TSTypeAssertion(inner) => &inner.expression,
                    _ => break,
                };
            }

            if let (Some(key), Some(value)) = (key, module_name(value)) {
                self.assignments.push(Assignment {
                    key,
                    value,
                    offset: it.span.start,
                });
            }
        }

        walk::walk_object_property(self, it);
    }
}

impl Tokens {
    fn heritage(&mut self, name: &TSTypeName<'_>) {
        if let TSTypeName::QualifiedName(qualified) = name {
            if let TSTypeName::IdentifierReference(object) = &qualified.left {
                self.members
                    .insert((object.name.to_string(), qualified.right.name.to_string()));
            }

            self.heritage(&qualified.left);
        }
    }
}

/// The text of a string literal or of a template literal without substitutions.
fn module_name(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::TemplateLiteral(template) => plain_template(template),
        _ => None,
    }
}

fn plain_template(template: &TemplateLiteral<'_>) -> Option<String> {
    match template.quasis.as_slice() {
        [quasi] if template.expressions.is_empty() => Some(
            quasi
                .value
                .cooked
                .map_or_else(String::new, |cooked| cooked.to_string()),
        ),
        _ => None,
    }
}

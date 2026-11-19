//! Native `TypeORM` options: which types and constructors take them, the object literals a value
//! materializes into, the writes that change `logging`, and whether an object ends with
//! `logging: false`.

use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    CallExpression, Expression, ObjectExpression, ObjectPropertyKind, PropertyKind,
    SimpleAssignmentTarget, TSType, UnaryOperator,
};
use gate_typescript::value::string_like;

use super::source::{Declaration, Seen, Source, key_name};
use super::{Form, form, unwrap};

impl<'a> Source<'_, 'a> {
    /// Whether a type is native `TypeORM` options: `DataSourceOptions`, `PostgresDataSourceOptions`,
    /// a local alias of one, or a library wrapper of one.
    pub fn native_options(&self, annotation: &'a TSType<'a>, seen: &mut Seen) -> bool {
        match annotation {
            TSType::TSParenthesizedType(inner) => self.native_options(&inner.type_annotation, seen),
            TSType::TSTypeReference(reference) => {
                if let Some(origin) = self.type_origin(&reference.type_name, &mut Seen::new())
                    && self
                        .names
                        .orm_options_types
                        .iter()
                        .any(|known| known.module == origin.module && known.name == origin.name)
                {
                    return true;
                }

                let gate_typescript::oxc_ast::ast::TSTypeName::IdentifierReference(name) =
                    &reference.type_name
                else {
                    return false;
                };

                match self.named(name) {
                    Some((key, Declaration::TypeAlias(alias))) => {
                        seen.insert(key) && self.native_options(&alias.type_annotation, seen)
                    }
                    Some(_) => false,
                    None => {
                        self.names
                            .wrapper_types
                            .iter()
                            .any(|wrapper| wrapper == name.name.as_str())
                            && reference
                                .type_arguments
                                .as_ref()
                                .and_then(|arguments| arguments.params.first())
                                .is_some_and(|argument| self.native_options(argument, seen))
                    }
                }
            }
            _ => false,
        }
    }

    pub fn data_source(&self, expression: &'a Expression<'a>) -> bool {
        self.native_constructor(
            expression,
            &self.names.orm_package,
            &self.names.orm_constructor,
            &mut Seen::new(),
        )
    }

    /// The object literals a value materializes into on each branch; `None` for a branch that is
    /// not a local object literal.
    pub fn objects(
        &self,
        expression: &'a Expression<'a>,
        seen: &mut Seen,
    ) -> Vec<Option<&'a ObjectExpression<'a>>> {
        let Some(value) = unwrap(expression) else {
            return vec![None];
        };

        match value {
            Expression::ObjectExpression(object) => vec![Some(object)],
            Expression::ConditionalExpression(conditional) => {
                let mut objects = self.objects(&conditional.consequent, &mut seen.clone());
                objects.extend(self.objects(&conditional.alternate, &mut seen.clone()));
                objects
            }
            Expression::Identifier(identifier) => {
                let Some((key, declaration)) = self.named(identifier) else {
                    return vec![None];
                };

                if !seen.insert(key) {
                    return vec![None];
                }

                match declaration.constant_initializer() {
                    Some(initializer) => self.objects(initializer, seen),
                    None => vec![None],
                }
            }
            _ => vec![None],
        }
    }

    /// The value whose `logging` a node writes: an assignment to, update or `delete` of its
    /// `logging` or unknown computed member, or an `Object.assign`, `Object.defineProperty` or
    /// `Reflect` write of it.
    pub fn logging_write(&self, kind: AstKind<'a>) -> Option<&'a Expression<'a>> {
        match kind {
            AstKind::AssignmentExpression(assignment) => simple(
                assignment.left.as_simple_assignment_target()?,
                &self.names.logging_property,
            ),
            AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
                member(unwrap(&unary.argument)?, &self.names.logging_property)
            }
            AstKind::UpdateExpression(update) => {
                simple(&update.argument, &self.names.logging_property)
            }
            AstKind::CallExpression(call) => self.reflected_write(call),
            _ => None,
        }
    }

    fn reflected_write(&self, call: &'a CallExpression<'a>) -> Option<&'a Expression<'a>> {
        let Form::Static(callee) = form(&call.callee) else {
            return None;
        };
        let Expression::Identifier(owner) = &callee.object else {
            return None;
        };

        if self.symbol(owner).is_some() {
            return None;
        }

        let target = || {
            call.arguments
                .first()
                .and_then(|argument| argument.as_expression())
        };

        match (owner.name.as_str(), callee.property.name.as_str()) {
            ("Object", "assign") => target(),
            ("Object", "defineProperty")
            | ("Reflect", "set" | "deleteProperty" | "defineProperty") => {
                let key = call
                    .arguments
                    .get(1)
                    .and_then(|argument| argument.as_expression())
                    .and_then(unwrap)
                    .and_then(string_like);

                if key.is_some_and(|key| key != self.names.logging_property) {
                    None
                } else {
                    target()
                }
            }
            _ => None,
        }
    }
}

/// The value whose `logging` a written target is a member of.
fn simple<'a>(
    target: &'a SimpleAssignmentTarget<'a>,
    property: &str,
) -> Option<&'a Expression<'a>> {
    match target {
        SimpleAssignmentTarget::StaticMemberExpression(target) => {
            (target.property.name == property).then_some(&target.object)
        }
        SimpleAssignmentTarget::ComputedMemberExpression(target) => {
            logging_key(&target.expression, property).then_some(&target.object)
        }
        SimpleAssignmentTarget::TSAsExpression(inner) => {
            member(unwrap(&inner.expression)?, property)
        }
        SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
            member(unwrap(&inner.expression)?, property)
        }
        SimpleAssignmentTarget::TSNonNullExpression(inner) => {
            member(unwrap(&inner.expression)?, property)
        }
        SimpleAssignmentTarget::TSTypeAssertion(inner) => {
            member(unwrap(&inner.expression)?, property)
        }
        _ => None,
    }
}

/// The value whose `logging` a written member expression is a member of.
fn member<'a>(target: &'a Expression<'a>, property: &str) -> Option<&'a Expression<'a>> {
    match form(target) {
        Form::Static(target) => (target.property.name == property).then_some(&target.object),
        Form::Computed(target) => {
            logging_key(&target.expression, property).then_some(&target.object)
        }
        _ => None,
    }
}

/// Whether a computed key may be `logging`: the literal or any key that is not a literal.
fn logging_key(key: &Expression<'_>, property: &str) -> bool {
    unwrap(key)
        .and_then(string_like)
        .is_none_or(|key| key == property)
}

/// Whether an object literal ends with `logging: false`: the last `logging` property is the literal
/// `false`, and no spread or name the compiler cannot read follows it.
pub fn final_false_logging(object: &ObjectExpression<'_>, logging: &str) -> bool {
    let mut disabled = false;

    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            disabled = false;
            continue;
        };

        match key_name(&property.key, property.computed) {
            None => disabled = false,
            Some(name) if name == logging => {
                disabled = property.kind == PropertyKind::Init
                    && !property.method
                    && !property.shorthand
                    && matches!(
                        unwrap(&property.value),
                        Some(Expression::BooleanLiteral(literal)) if !literal.value
                    );
            }
            Some(_) => {}
        }
    }

    disabled
}

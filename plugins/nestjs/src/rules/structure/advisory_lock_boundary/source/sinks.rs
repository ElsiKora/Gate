//! The SQL sinks of a source and the statements a canonical executor argument may run.

use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{ChainElement, Expression, PropertyKey};

use super::super::sql::{Text, units};
use super::evidence::File;
use super::interpreter::Interpreter;
use super::node::{Node, argument_node, equals, string_units};
use super::text::{Candidates, consume};
use super::{Outcome, Seen};

/// A SQL sink a node names: the arguments a `bind` fixes and whether the call still takes its SQL.
pub(super) struct Binding<'a> {
    pub(super) arguments: Vec<Node<'a>>,
    pub(super) unbound: bool,
}

impl<'a> File<'a> {
    /// The SQL sink a callee or tag names.
    pub(super) fn binding(
        &self,
        node: Node<'a>,
        methods: &[String],
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Option<Binding<'a>>> {
        consume(budget, depth)?;
        let mut expression = node;

        while let Some(inner) = expression.typed_operand() {
            expression = inner;
        }

        let named = |name: &[u16]| methods.iter().any(|method| equals(name, method.as_str()));

        if let Node::Binding(binding) = expression {
            let key = match &binding.key {
                PropertyKey::StaticIdentifier(identifier) => Some(units(identifier.name.as_str())),
                PropertyKey::StringLiteral(literal) if !binding.computed => Some(string_units(
                    literal.value.as_str(),
                    literal.lone_surrogates,
                )),
                _ => None,
            };

            if key.is_some_and(|key| named(&key)) {
                return Ok(Some(Binding {
                    arguments: Vec::new(),
                    unbound: true,
                }));
            }
        }

        if let Some((_, name)) = Interpreter::property_reference(expression)
            && named(&name)
        {
            return Ok(Some(Binding {
                arguments: Vec::new(),
                unbound: true,
            }));
        }

        let computed = match expression {
            Node::Expression(Expression::ComputedMemberExpression(member)) => {
                Some(&member.expression)
            }
            Node::Expression(Expression::ChainExpression(chain)) => match &chain.expression {
                ChainElement::ComputedMemberExpression(member) => Some(&member.expression),
                _ => None,
            },
            _ => None,
        };

        if let Some(argument) = computed
            && self
                .interpreter
                .fresh_candidates(Node::Expression(argument))?
                .iter()
                .any(|name| named(name))
        {
            return Ok(Some(Binding {
                arguments: Vec::new(),
                unbound: true,
            }));
        }

        if let Some((call, name, receiver)) = Interpreter::method_call(expression)
            && equals(&name, "bind")
        {
            let binding = self.binding(receiver, methods, seen, budget, depth + 1)?;

            if let Some(binding) = &binding
                && binding.unbound
                && let Some(argument) = call.arguments.get(1)
            {
                let mut arguments = binding.arguments.clone();
                arguments.push(argument_node(argument));

                return Ok(Some(Binding {
                    arguments,
                    unbound: false,
                }));
            }

            return Ok(binding);
        }

        let mut seen = seen.clone();
        let branches: Option<Vec<Node<'a>>> =
            if let Some(name) = expression.identifier().filter(|name| !seen.contains(name)) {
                seen.insert(name);
                self.interpreter.variables.get(name).cloned()
            } else {
                expression.branches().map(|branches| branches.to_vec())
            };

        if let Some(branches) = branches {
            let mut bindings = Vec::new();

            for branch in branches {
                if let Some(binding) = self.binding(branch, methods, &seen, budget, depth + 1)? {
                    bindings.push(binding);
                }
            }

            if !bindings.is_empty() {
                return Ok(Some(Binding {
                    arguments: bindings
                        .iter()
                        .flat_map(|binding| binding.arguments.clone())
                        .collect(),
                    unbound: bindings.iter().any(|binding| binding.unbound),
                }));
            }
        }

        Ok(None)
    }

    /// The SQL arguments of a call or tagged template.
    pub(super) fn sql_arguments(&self, kind: &AstKind<'a>) -> Outcome<Vec<Node<'a>>> {
        match kind {
            AstKind::TaggedTemplateExpression(tagged) => {
                let Some(binding) = self.binding(
                    Node::Expression(&tagged.tag),
                    &self.sinks.tags,
                    &Seen::new(),
                    &mut 0,
                    0,
                )?
                else {
                    return Ok(Vec::new());
                };
                let mut arguments = binding.arguments;

                if binding.unbound {
                    arguments.push(Node::Template(&tagged.quasi));
                }

                Ok(arguments)
            }
            AstKind::CallExpression(call) => {
                let mut binding = self.binding(
                    Node::Expression(&call.callee),
                    &self.sinks.methods,
                    &Seen::new(),
                    &mut 0,
                    0,
                )?;
                let mut argument = call.arguments.first().map(argument_node);

                if binding.is_none()
                    && let Some((receiver, name)) =
                        Interpreter::property_reference(Node::Expression(&call.callee))
                    && (equals(&name, "call") || equals(&name, "apply"))
                {
                    binding =
                        self.binding(receiver, &self.sinks.methods, &Seen::new(), &mut 0, 0)?;
                    argument = call.arguments.get(1).map(argument_node);

                    if equals(&name, "apply")
                        && let Some(applied) = argument
                    {
                        argument = Some(
                            self.interpreter
                                .array_elements(applied, &mut Seen::new(), &mut 0, 0)?
                                .and_then(|elements| elements.first().copied())
                                .unwrap_or(applied),
                        );
                    }
                }

                let Some(binding) = binding else {
                    return Ok(Vec::new());
                };
                let mut arguments = binding.arguments;

                if binding.unbound
                    && let Some(argument) = argument
                {
                    arguments.push(argument);
                }

                Ok(arguments)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Every statement a canonical executor argument may run, or `None` when one cannot be rebuilt.
    pub(super) fn complete_candidates(
        &self,
        node: Node<'a>,
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Option<Vec<Text>>> {
        consume(budget, depth)?;

        if let Some(statement) = self.interpreter.fresh_text(node)? {
            return Ok(Some(vec![statement]));
        }

        if let Some(inner) = node.typed_operand() {
            return self.complete_candidates(inner, seen, budget, depth + 1);
        }

        let mut seen = seen.clone();
        let branches: Vec<Node<'a>> =
            if let Some(name) = node.identifier().filter(|name| !seen.contains(name)) {
                seen.insert(name);
                self.interpreter.get(name).to_vec()
            } else if let Node::Expression(Expression::ConditionalExpression(conditional)) = node {
                vec![
                    Node::Expression(&conditional.consequent),
                    Node::Expression(&conditional.alternate),
                ]
            } else {
                Vec::new()
            };

        if branches.is_empty() {
            return Ok(None);
        }

        let mut candidates = Candidates::default();

        for branch in branches {
            let Some(statements) = self.complete_candidates(branch, &seen, budget, depth + 1)?
            else {
                return Ok(None);
            };

            for statement in statements {
                candidates.add(statement, budget, depth)?;
            }
        }

        Ok(Some(candidates.0))
    }
}

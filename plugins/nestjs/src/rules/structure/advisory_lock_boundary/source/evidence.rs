//! The advisory evidence of a sink argument that cannot be rebuilt: the file's fragment and
//! mutated variables and the evidence search behind variables.

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BindingPattern, ChainElement, Expression,
    ObjectExpression, ObjectPropertyKind, PropertyKey, SimpleAssignmentTarget,
};
use gate_typescript::oxc_span::GetSpan;
use rustc_hash::FxHashSet as HashSet;

use super::super::sql::Text;
use super::interpreter::Interpreter;
use super::node::{Node, argument_node, element_node};
use super::text::{WORK_LIMIT, is_advisory, is_declaration_fragment, is_fragment};
use super::{Fail, Outcome, Seen, SqlSinks};

/// The direct children of a node the advisory-evidence search reads, as `ts.forEachChild` yields
/// them. A node the search does not model stands as an opaque span; identifiers anywhere inside a
/// node are followed by [`File::contains_advisory`] itself.
pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let expression = |expression| Node::Expression(expression);

    match node {
        Node::Spread(spread) => vec![expression(&spread.argument)],
        Node::Template(template) => template.expressions.iter().map(expression).collect(),
        Node::Binding(binding) => {
            let mut nodes = Vec::new();

            match &binding.key {
                PropertyKey::StaticIdentifier(identifier) => {
                    nodes.push(Node::Name(identifier.name.as_str(), identifier.span));
                }
                key => nodes.extend(key.as_expression().map(expression)),
            }

            match &binding.value {
                BindingPattern::BindingIdentifier(identifier) => {
                    nodes.push(Node::Name(identifier.name.as_str(), identifier.span));
                }
                BindingPattern::AssignmentPattern(pattern) => {
                    if let BindingPattern::BindingIdentifier(identifier) = &pattern.left {
                        nodes.push(Node::Name(identifier.name.as_str(), identifier.span));
                    } else {
                        nodes.push(Node::Opaque(pattern.left.span()));
                    }
                    nodes.push(expression(&pattern.right));
                }
                pattern => nodes.push(Node::Opaque(pattern.span())),
            }

            nodes
        }
        Node::Omitted(_) | Node::Name(..) | Node::Opaque(_) => Vec::new(),
        Node::Expression(value) => expression_children(value),
    }
}

/// The names, keys and values of an object literal's properties.
fn object_children<'a>(object: &'a ObjectExpression<'a>) -> Vec<Node<'a>> {
    let mut nodes = Vec::new();

    for property in &object.properties {
        match property {
            ObjectPropertyKind::ObjectProperty(property) => {
                match &property.key {
                    PropertyKey::StaticIdentifier(identifier) if !property.shorthand => {
                        nodes.push(Node::Name(identifier.name.as_str(), identifier.span));
                    }
                    PropertyKey::StaticIdentifier(_) => {}
                    key => nodes.extend(key.as_expression().map(Node::Expression)),
                }
                nodes.push(Node::Expression(&property.value));
            }
            ObjectPropertyKind::SpreadProperty(spread) => nodes.push(Node::Spread(spread)),
        }
    }

    nodes
}

/// The children of a simple assignment target: a name, or the parts of a member access.
fn target_children<'a>(target: &'a SimpleAssignmentTarget<'a>) -> Vec<Node<'a>> {
    let expression = |expression| Node::Expression(expression);

    match target {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
            vec![Node::Name(identifier.name.as_str(), identifier.span)]
        }
        SimpleAssignmentTarget::StaticMemberExpression(member) => vec![
            expression(&member.object),
            Node::Name(member.property.name.as_str(), member.property.span),
        ],
        SimpleAssignmentTarget::ComputedMemberExpression(member) => {
            vec![expression(&member.object), expression(&member.expression)]
        }
        SimpleAssignmentTarget::PrivateFieldExpression(member) => vec![expression(&member.object)],
        SimpleAssignmentTarget::TSAsExpression(inner) => vec![expression(&inner.expression)],
        SimpleAssignmentTarget::TSSatisfiesExpression(inner) => vec![expression(&inner.expression)],
        SimpleAssignmentTarget::TSNonNullExpression(inner) => vec![expression(&inner.expression)],
        SimpleAssignmentTarget::TSTypeAssertion(inner) => vec![expression(&inner.expression)],
    }
}

pub(super) fn expression_children<'a>(value: &'a Expression<'a>) -> Vec<Node<'a>> {
    let expression = |expression| Node::Expression(expression);

    match value {
        Expression::TemplateLiteral(template) => {
            template.expressions.iter().map(expression).collect()
        }
        Expression::TaggedTemplateExpression(tagged) => {
            vec![expression(&tagged.tag), Node::Template(&tagged.quasi)]
        }
        Expression::ArrayExpression(array) => array.elements.iter().map(element_node).collect(),
        Expression::ObjectExpression(object) => object_children(object),
        Expression::CallExpression(call) => {
            let mut nodes = vec![expression(&call.callee)];
            nodes.extend(call.arguments.iter().map(argument_node));
            nodes
        }
        Expression::NewExpression(call) => {
            let mut nodes = vec![expression(&call.callee)];
            nodes.extend(call.arguments.iter().map(argument_node));
            nodes
        }
        Expression::StaticMemberExpression(member) => vec![
            expression(&member.object),
            Node::Name(member.property.name.as_str(), member.property.span),
        ],
        Expression::ComputedMemberExpression(member) => {
            vec![expression(&member.object), expression(&member.expression)]
        }
        Expression::PrivateFieldExpression(member) => vec![expression(&member.object)],
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => {
                let mut nodes = vec![expression(&call.callee)];
                nodes.extend(call.arguments.iter().map(argument_node));
                nodes
            }
            ChainElement::StaticMemberExpression(member) => vec![
                expression(&member.object),
                Node::Name(member.property.name.as_str(), member.property.span),
            ],
            ChainElement::ComputedMemberExpression(member) => {
                vec![expression(&member.object), expression(&member.expression)]
            }
            ChainElement::PrivateFieldExpression(member) => vec![expression(&member.object)],
            ChainElement::TSNonNullExpression(inner) => vec![expression(&inner.expression)],
        },
        Expression::BinaryExpression(binary) => {
            vec![expression(&binary.left), expression(&binary.right)]
        }
        Expression::LogicalExpression(logical) => {
            vec![expression(&logical.left), expression(&logical.right)]
        }
        Expression::ConditionalExpression(conditional) => vec![
            expression(&conditional.test),
            expression(&conditional.consequent),
            expression(&conditional.alternate),
        ],
        Expression::SequenceExpression(sequence) => {
            sequence.expressions.iter().map(expression).collect()
        }
        Expression::UnaryExpression(unary) => vec![expression(&unary.argument)],
        Expression::AwaitExpression(inner) => vec![expression(&inner.argument)],
        Expression::YieldExpression(inner) => inner.argument.iter().map(expression).collect(),
        Expression::ParenthesizedExpression(inner) => vec![expression(&inner.expression)],
        Expression::TSAsExpression(inner) => vec![expression(&inner.expression)],
        Expression::TSSatisfiesExpression(inner) => vec![expression(&inner.expression)],
        Expression::TSNonNullExpression(inner) => vec![expression(&inner.expression)],
        Expression::TSTypeAssertion(inner) => vec![expression(&inner.expression)],
        Expression::AssignmentExpression(assignment) => {
            let mut nodes = assignment.left.as_simple_assignment_target().map_or_else(
                || vec![Node::Opaque(assignment.left.span())],
                target_children,
            );
            nodes.push(expression(&assignment.right));
            nodes
        }
        Expression::UpdateExpression(update) => target_children(&update.argument),
        Expression::ImportExpression(import) => {
            let mut nodes = vec![expression(&import.source)];
            nodes.extend(import.options.iter().map(expression));
            nodes
        }
        Expression::TSInstantiationExpression(inner) => vec![expression(&inner.expression)],
        Expression::PrivateInExpression(inner) => vec![expression(&inner.right)],
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::Identifier(_)
        | Expression::ThisExpression(_)
        | Expression::Super(_) => Vec::new(),
        // Function and class bodies, JSX and any other node are not modelled: they fail closed
        // where their text holds an advisory fragment.
        _ => vec![Node::Opaque(value.span())],
    }
}

/// The file-level reading of one parsed source: its variables, the advisory-fragment variables and
/// the fragment variables an assignment the reconstruction cannot follow changes.
pub(super) struct File<'a> {
    pub(super) sinks: SqlSinks,
    pub(super) interpreter: Interpreter<'a>,
    pub(super) fragments: HashSet<&'a str>,
    pub(super) mutated: HashSet<&'a str>,
}

impl<'a> File<'a> {
    pub(super) fn new(file: &SourceFile<'a>, sinks: SqlSinks) -> Outcome<Self> {
        let interpreter = Interpreter::new(file);
        let fragments = Self::fragments(&interpreter)?;
        let mut unreconstructible = HashSet::default();

        for node in file.semantic().nodes().iter() {
            if let AstKind::AssignmentExpression(assignment) = node.kind()
                && let AssignmentTarget::AssignmentTargetIdentifier(identifier) = &assignment.left
                && (assignment.operator != AssignmentOperator::Assign
                    || interpreter
                        .fresh_candidates(Node::Expression(&assignment.right))?
                        .is_empty())
            {
                unreconstructible.insert(identifier.name.as_str());
            }
        }

        let seeds: HashSet<&'a str> = unreconstructible
            .into_iter()
            .filter(|name| fragments.contains(name))
            .collect();
        let mutated = Self::dependents(&interpreter, seeds);

        Ok(Self {
            sinks,
            interpreter,
            fragments,
            mutated,
        })
    }

    /// The variables whose value may hold an advisory fragment, directly or through another.
    pub(super) fn fragments(interpreter: &Interpreter<'a>) -> Outcome<HashSet<&'a str>> {
        let mut fragments = HashSet::default();
        let mut dependencies: Vec<(&'a str, HashSet<&'a str>)> = Vec::new();

        for &name in &interpreter.order {
            let mut names = HashSet::default();

            for &initializer in interpreter.get(name) {
                let candidates = interpreter.fresh_candidates(initializer)?;

                if is_fragment(&interpreter.source(initializer))
                    || candidates.iter().any(|candidate| is_fragment(candidate))
                {
                    fragments.insert(name);
                }

                names.extend(interpreter.names(initializer));
            }

            dependencies.push((name, names));
        }

        let mut changed = true;

        while changed {
            changed = false;

            for (name, names) in &dependencies {
                if !fragments.contains(name)
                    && names
                        .iter()
                        .any(|dependency| fragments.contains(dependency))
                {
                    fragments.insert(name);
                    changed = true;
                }
            }
        }

        Ok(fragments)
    }

    /// The seeds and every variable whose initializer names one of them, transitively.
    pub(super) fn dependents(
        interpreter: &Interpreter<'a>,
        seeds: HashSet<&'a str>,
    ) -> HashSet<&'a str> {
        let mut dependents = seeds;
        let mut changed = true;

        while changed {
            changed = false;

            for &name in &interpreter.order {
                if !dependents.contains(name)
                    && interpreter.get(name).iter().any(|initializer| {
                        interpreter
                            .names(*initializer)
                            .iter()
                            .any(|dependency| dependents.contains(dependency))
                    })
                {
                    dependents.insert(name);
                    changed = true;
                }
            }
        }

        dependents
    }

    pub(super) fn contains_fragment(&self, node: Node<'a>) -> bool {
        is_fragment(&self.interpreter.source(node))
            || self
                .interpreter
                .names(node)
                .iter()
                .any(|name| self.fragments.contains(name))
    }

    /// Whether a sink argument carries advisory evidence in its text, its reconstruction or the
    /// variables it reads: the node is walked, and every
    /// identifier inside its span follows its assignments, wherever the walk reaches it or not.
    ///
    /// # Errors
    /// Fails with the resource message once the walk of one argument passes the work budget.
    pub(super) fn contains_advisory(&self, node: Node<'a>) -> Outcome<bool> {
        let interpreter = &self.interpreter;

        // A function or class declaration the argument itself names is read as its text alone,
        // gated by an advisory fragment other than a bare `pg_`. Its body's identifiers are not
        // followed, and a declaration named only behind a variable is not read: names are matched
        // by text across the file, so either would reach every function the file declares.
        for name in interpreter.names(node) {
            for span in interpreter.declarations.get(name).into_iter().flatten() {
                if is_declaration_fragment(&interpreter.source(Node::Opaque(*span))) {
                    return Ok(true);
                }
            }
        }

        self.evidence(node, &Seen::default(), &mut Seen::default(), &mut 0)
    }

    /// The evidence search of one node. `path` guards the reconstruction against cycles: it is
    /// empty for the argument and holds the followed name for that name's initializer; `followed`
    /// holds every name the search of the argument followed, so that each is followed once and the
    /// search stays linear.
    fn evidence(
        &self,
        node: Node<'a>,
        path: &Seen<'a>,
        followed: &mut Seen<'a>,
        work: &mut u32,
    ) -> Outcome<bool> {
        if self.walk(node, path, work)? {
            return Ok(true);
        }

        let interpreter = &self.interpreter;
        let mut names: Vec<&'a str> = interpreter.names(node).into_iter().collect();
        names.sort_unstable();

        for name in names {
            let initializers = interpreter.get(name);

            if initializers.is_empty() || !followed.insert(name) {
                continue;
            }

            // The initializer is read with its own name alone as the cycle guard, not with the
            // names on the way to it: its result must not depend on the path that reached it
            // first, or following it once would lose what another path finds.
            let next: Seen<'a> = std::iter::once(name).collect();

            for initializer in initializers {
                if self.evidence(*initializer, &next, followed, work)? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// The text, reconstruction and children of a node; an opaque node is evidence where its text
    /// holds an advisory fragment.
    fn walk(&self, node: Node<'a>, seen: &Seen<'a>, work: &mut u32) -> Outcome<bool> {
        let interpreter = &self.interpreter;
        *work += 1;

        if *work > WORK_LIMIT {
            return Err(Fail::resource());
        }

        if let Node::Opaque(_) = node {
            return Ok(is_fragment(&interpreter.source(node)));
        }

        let reconstructed = if self.contains_fragment(node) {
            interpreter.candidates(node, &seen.clone(), &mut 0, 0)?
        } else {
            Vec::new()
        };

        if is_advisory(&interpreter.source(node))
            || reconstructed.iter().any(|text| is_advisory(text))
        {
            return Ok(true);
        }

        if let Node::Expression(Expression::ArrayExpression(_)) = node
            && let Some(elements) =
                interpreter.array_elements(node, &mut seen.clone(), &mut 0, 0)?
        {
            let mut parts = Vec::new();

            for element in elements {
                parts.push(interpreter.text(element, &mut seen.clone(), &mut 0, 0)?);
            }

            if let Some(parts) = parts.into_iter().collect::<Option<Vec<Text>>>()
                && is_advisory(&parts.join(&u16::from(b',')))
            {
                return Ok(true);
            }
        }

        for child in children(node) {
            if self.walk(child, seen, work)? {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

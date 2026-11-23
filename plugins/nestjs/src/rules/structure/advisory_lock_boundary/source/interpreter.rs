//! The static reconstruction of one source: its variables and the texts an expression may hold.

use std::cell::Cell;

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentTarget, BindingPattern, CallExpression,
    ChainElement, Expression, ObjectPropertyKind, PropertyKey, PropertyKind,
};
use gate_typescript::oxc_span::Span;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use super::super::sql::{Text, units};
use super::node::{
    Node, argument_node, element_node, equals, literal, quasi, string_units, unwrap_static,
};
use super::text::{
    CANDIDATE_LIMIT, Candidates, DEPTH_LIMIT, INTERPOLATION, concatenate, concatenate_candidates,
    consume, joined, replace, validate, validate_replacement,
};
use super::{Fail, Outcome, Seen};

/// The variables, identifiers and advisory-fragment variables of one parsed source.
pub(super) struct Interpreter<'a> {
    pub(super) text: &'a str,
    pub(super) order: Vec<&'a str>,
    pub(super) variables: HashMap<&'a str, Vec<Node<'a>>>,
    /// Every identifier-like name of the file with its span, in source order.
    pub(super) identifiers: Vec<(Span, &'a str)>,
    /// The spans of the function and class declarations of each name, which the advisory-evidence
    /// search reads as opaque initializers and the reconstruction does not.
    pub(super) declarations: HashMap<&'a str, Vec<Span>>,
    /// Whether a reconstruction since the last reset met an operation on a text it rebuilt that it
    /// cannot evaluate: a `replace` whose search or replacement it does not rebuild (a regular
    /// expression or a callback, for one), or a method it does not model.
    pub(super) unrebuilt: Cell<bool>,
}

impl<'a> Interpreter<'a> {
    pub(super) fn new(file: &SourceFile<'a>) -> Self {
        let mut interpreter = Self {
            text: file.source(),
            order: Vec::new(),
            variables: HashMap::default(),
            identifiers: Vec::new(),
            declarations: HashMap::default(),
            unrebuilt: Cell::new(false),
        };

        for node in file.semantic().nodes().iter() {
            match node.kind() {
                AstKind::BindingProperty(property) => {
                    let name = match &property.value {
                        BindingPattern::BindingIdentifier(identifier) => {
                            Some(identifier.name.as_str())
                        }
                        BindingPattern::AssignmentPattern(pattern) => match &pattern.left {
                            BindingPattern::BindingIdentifier(identifier) => {
                                Some(identifier.name.as_str())
                            }
                            _ => None,
                        },
                        _ => None,
                    };

                    if let Some(name) = name {
                        interpreter.push(name, Node::Binding(property));
                    }
                }
                AstKind::VariableDeclarator(declarator) => {
                    if let (BindingPattern::BindingIdentifier(identifier), Some(initializer)) =
                        (&declarator.id, &declarator.init)
                    {
                        interpreter.push(identifier.name.as_str(), Node::Expression(initializer));
                    }
                }
                AstKind::AssignmentExpression(assignment) => {
                    if let AssignmentTarget::AssignmentTargetIdentifier(identifier) =
                        &assignment.left
                    {
                        interpreter.push(
                            identifier.name.as_str(),
                            Node::Expression(&assignment.right),
                        );
                    }
                }
                AstKind::Function(function) if function.is_declaration() => {
                    if let Some(identifier) = &function.id {
                        interpreter
                            .declarations
                            .entry(identifier.name.as_str())
                            .or_default()
                            .push(function.span);
                    }
                }
                AstKind::Class(class) if class.is_declaration() => {
                    if let Some(identifier) = &class.id {
                        interpreter
                            .declarations
                            .entry(identifier.name.as_str())
                            .or_default()
                            .push(class.span);
                    }
                }
                AstKind::IdentifierName(identifier) => {
                    interpreter
                        .identifiers
                        .push((identifier.span, identifier.name.as_str()));
                }
                AstKind::IdentifierReference(identifier) => {
                    interpreter
                        .identifiers
                        .push((identifier.span, identifier.name.as_str()));
                }
                AstKind::BindingIdentifier(identifier) => {
                    interpreter
                        .identifiers
                        .push((identifier.span, identifier.name.as_str()));
                }
                AstKind::LabelIdentifier(identifier) => {
                    interpreter
                        .identifiers
                        .push((identifier.span, identifier.name.as_str()));
                }
                AstKind::JSXIdentifier(identifier) => {
                    interpreter
                        .identifiers
                        .push((identifier.span, identifier.name.as_str()));
                }
                _ => {}
            }
        }

        interpreter
            .identifiers
            .sort_by_key(|(span, _)| (span.start, span.end));
        interpreter
    }

    pub(super) fn push(&mut self, name: &'a str, initializer: Node<'a>) {
        let initializers = self.variables.entry(name).or_default();

        if initializers.is_empty() {
            self.order.push(name);
        }

        initializers.push(initializer);
    }

    pub(super) fn get(&self, name: &str) -> &[Node<'a>] {
        self.variables.get(name).map_or(&[], Vec::as_slice)
    }

    pub(super) fn unique(&self, name: &str) -> Option<Node<'a>> {
        match self.get(name) {
            [initializer] => Some(*initializer),
            _ => None,
        }
    }

    /// The source text of a node.
    pub(super) fn source(&self, node: Node<'a>) -> Text {
        let span = node.span();

        self.text
            .get(span.start as usize..span.end as usize)
            .map_or_else(Text::new, units)
    }

    /// Every identifier-like name inside a node, the node itself included.
    pub(super) fn names(&self, node: Node<'a>) -> HashSet<&'a str> {
        let span = node.span();
        let first = self
            .identifiers
            .partition_point(|(inner, _)| inner.start < span.start);
        let mut names: HashSet<&'a str> = self.identifiers[first..]
            .iter()
            .take_while(|(inner, _)| inner.start < span.end)
            .filter(|(inner, _)| inner.end <= span.end)
            .map(|(_, name)| *name)
            .collect();

        if let Some(name) = node.identifier() {
            names.insert(name);
        }

        names
    }

    /// The object and property name of a property access.
    pub(super) fn property_reference(node: Node<'a>) -> Option<(Node<'a>, Text)> {
        let node = unwrap_static(node);
        let member = match node {
            Node::Expression(Expression::ChainExpression(chain)) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => {
                    return Some((
                        unwrap_static(Node::Expression(&member.object)),
                        units(member.property.name.as_str()),
                    ));
                }
                ChainElement::ComputedMemberExpression(member) => Some(member.as_ref()),
                ChainElement::PrivateFieldExpression(member) => {
                    return Some((
                        unwrap_static(Node::Expression(&member.object)),
                        units(&format!("#{}", member.field.name)),
                    ));
                }
                _ => None,
            },
            Node::Expression(Expression::StaticMemberExpression(member)) => {
                return Some((
                    unwrap_static(Node::Expression(&member.object)),
                    units(member.property.name.as_str()),
                ));
            }
            Node::Expression(Expression::PrivateFieldExpression(member)) => {
                return Some((
                    unwrap_static(Node::Expression(&member.object)),
                    units(&format!("#{}", member.field.name)),
                ));
            }
            Node::Expression(Expression::ComputedMemberExpression(member)) => Some(member.as_ref()),
            _ => None,
        }?;
        let name = match &member.expression {
            Expression::StringLiteral(literal) => {
                string_units(literal.value.as_str(), literal.lone_surrogates)
            }
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                quasi(template, 0)
            }
            _ => return None,
        };

        Some((unwrap_static(Node::Expression(&member.object)), name))
    }

    /// The method name and receiver of a call of a property.
    pub(super) fn method_call(node: Node<'a>) -> Option<(&'a CallExpression<'a>, Text, Node<'a>)> {
        let call = node.call()?;
        let (receiver, name) = Self::property_reference(Node::Expression(&call.callee))?;

        Some((call, name, receiver))
    }

    /// The object-literal values a property access reads.
    pub(super) fn property_initializers(&self, node: Node<'a>) -> Vec<Node<'a>> {
        let Some((owner, name)) = Self::property_reference(node) else {
            return Vec::new();
        };
        let owners: Vec<Node<'a>> = match owner.identifier() {
            Some(identifier) => self.get(identifier).to_vec(),
            None => vec![owner],
        };
        let mut initializers = Vec::new();

        for owner in owners {
            let Node::Expression(Expression::ObjectExpression(object)) = unwrap_static(owner)
            else {
                continue;
            };

            for property in &object.properties {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    continue;
                };

                if property.shorthand
                    || property.method
                    || property.computed
                    || property.kind != PropertyKind::Init
                {
                    continue;
                }

                let key = match &property.key {
                    PropertyKey::StaticIdentifier(identifier) => units(identifier.name.as_str()),
                    PropertyKey::StringLiteral(literal) => {
                        string_units(literal.value.as_str(), literal.lone_surrogates)
                    }
                    _ => continue,
                };

                if key == name {
                    initializers.push(Node::Expression(&property.value));
                }
            }
        }

        initializers
    }

    pub(super) fn array_elements(
        &self,
        node: Node<'a>,
        seen: &mut Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Option<Vec<Node<'a>>>> {
        consume(budget, depth)?;

        if let Some(inner) = node.parenthesized_or_as() {
            return self.array_elements(inner, seen, budget, depth + 1);
        }

        if let Node::Expression(Expression::ArrayExpression(array)) = node {
            let mut flattened = Vec::new();

            for element in &array.elements {
                if let ArrayExpressionElement::SpreadElement(spread) = element {
                    let Some(elements) = self.array_elements(
                        Node::Expression(&spread.argument),
                        &mut seen.clone(),
                        budget,
                        depth + 1,
                    )?
                    else {
                        return Ok(None);
                    };
                    flattened.extend(elements);
                } else {
                    flattened.push(element_node(element));
                }
            }

            return Ok(Some(flattened));
        }

        if let Some((call, name, receiver)) = Self::method_call(node)
            && equals(&name, "concat")
        {
            let Some(mut concatenated) =
                self.array_elements(receiver, &mut seen.clone(), budget, depth + 1)?
            else {
                return Ok(None);
            };
            let mut arguments = Vec::new();

            for argument in &call.arguments {
                if let Argument::SpreadElement(spread) = argument {
                    let Some(elements) = self.array_elements(
                        Node::Expression(&spread.argument),
                        &mut seen.clone(),
                        budget,
                        depth + 1,
                    )?
                    else {
                        return Ok(None);
                    };
                    arguments.extend(elements);
                } else {
                    arguments.push(argument_node(argument));
                }
            }

            for argument in arguments {
                match self.array_elements(argument, &mut seen.clone(), budget, depth + 1)? {
                    None => concatenated.push(argument),
                    Some(elements) => concatenated.extend(elements),
                }
            }

            return Ok(Some(concatenated));
        }

        if let Some(name) = node.identifier().filter(|name| !seen.contains(name))
            && let Some(initializer) = self.unique(name)
        {
            seen.insert(name);

            return self.array_elements(initializer, seen, budget, depth + 1);
        }

        if let [initializer] = self.property_initializers(node).as_slice() {
            return self.array_elements(*initializer, &mut seen.clone(), budget, depth + 1);
        }

        Ok(None)
    }

    pub(super) fn call_arguments(
        &self,
        arguments: &'a [Argument<'a>],
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Option<Vec<Node<'a>>>> {
        let mut nodes = Vec::new();

        for argument in arguments {
            if let Argument::SpreadElement(spread) = argument {
                let Some(elements) = self.array_elements(
                    Node::Expression(&spread.argument),
                    &mut seen.clone(),
                    budget,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                nodes.extend(elements);
            } else {
                nodes.push(argument_node(argument));
            }
        }

        Ok(Some(nodes))
    }

    pub(super) fn array_element_candidates(
        &self,
        node: Node<'a>,
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Vec<Vec<Node<'a>>>> {
        consume(budget, depth)?;
        let expression = unwrap_static(node);

        if let Some(name) = expression.identifier().filter(|name| !seen.contains(name)) {
            let mut next = seen.clone();
            next.insert(name);
            let mut candidates = Vec::new();

            for initializer in self.get(name) {
                candidates.extend(self.array_element_candidates(
                    *initializer,
                    &next,
                    budget,
                    depth + 1,
                )?);

                if candidates.len() > CANDIDATE_LIMIT {
                    return Err(Fail::new("Static array candidate budget was exceeded."));
                }
            }

            return Ok(candidates);
        }

        let initializers = self.property_initializers(expression);

        if !initializers.is_empty() {
            let mut candidates = Vec::new();

            for initializer in initializers {
                candidates.extend(self.array_element_candidates(
                    initializer,
                    seen,
                    budget,
                    depth + 1,
                )?);

                if candidates.len() > CANDIDATE_LIMIT {
                    return Err(Fail::new("Static array candidate budget was exceeded."));
                }
            }

            return Ok(candidates);
        }

        Ok(self
            .array_elements(expression, &mut seen.clone(), budget, depth + 1)?
            .map_or_else(Vec::new, |elements| vec![elements]))
    }

    /// The literal text a node holds through parentheses, `as`, tagged templates, single-assigned
    /// variables and object properties, with the number of steps to it.
    pub(super) fn atomic(
        &self,
        node: Node<'a>,
        seen: &Seen<'a>,
        depth: usize,
    ) -> Outcome<Option<(usize, Text)>> {
        if depth > DEPTH_LIMIT {
            return Err(Fail::resource());
        }

        if let Some(text) = literal(node) {
            return Ok(Some((0, text)));
        }

        let step = |result: Option<(usize, Text)>| result.map(|(hops, text)| (hops + 1, text));

        if let Some(inner) = node.parenthesized_or_as() {
            return Ok(step(self.atomic(inner, seen, depth + 1)?));
        }

        if let Node::Expression(Expression::TaggedTemplateExpression(tagged)) = node {
            return Ok(step(self.atomic(
                Node::Template(&tagged.quasi),
                seen,
                depth + 1,
            )?));
        }

        if let Some(name) = node.identifier().filter(|name| !seen.contains(name))
            && let Some(initializer) = self.unique(name)
        {
            let mut next = seen.clone();
            next.insert(name);

            return Ok(step(self.atomic(initializer, &next, depth + 1)?));
        }

        match self.property_initializers(node).as_slice() {
            [initializer] => Ok(step(self.atomic(*initializer, seen, depth + 1)?)),
            _ => Ok(None),
        }
    }

    pub(super) fn consume_atomic(hops: usize, budget: &mut u32, depth: usize) -> Outcome<()> {
        for hop in 1..=hops {
            consume(budget, depth + hop)?;
        }

        Ok(())
    }

    /// The one text a node evaluates to, or `None` when it cannot be rebuilt statically.
    #[expect(
        clippy::too_many_lines,
        reason = "one reconstruction keeps the order of its branches"
    )]
    pub(super) fn text(
        &self,
        node: Node<'a>,
        seen: &mut Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Option<Text>> {
        consume(budget, depth)?;

        if let Some((hops, text)) = self.atomic(node, seen, depth)? {
            Self::consume_atomic(hops, budget, depth)?;

            return Ok(Some(text));
        }

        if let Node::Omitted(_) = node {
            return Ok(Some(Text::new()));
        }

        if let Some(inner) = node.parenthesized_or_as() {
            return self.text(inner, seen, budget, depth + 1);
        }

        if let Node::Expression(Expression::TaggedTemplateExpression(tagged)) = node {
            return self.text(
                Node::Template(&tagged.quasi),
                &mut seen.clone(),
                budget,
                depth + 1,
            );
        }

        if let Node::Expression(Expression::ArrayExpression(_)) = node {
            let Some(elements) = self.array_elements(node, &mut seen.clone(), budget, depth + 1)?
            else {
                return Ok(None);
            };
            let mut parts = Vec::new();

            for element in elements {
                parts.push(self.text(element, &mut seen.clone(), budget, depth + 1)?);
            }

            let Some(parts) = parts.into_iter().collect::<Option<Vec<Text>>>() else {
                return Ok(None);
            };

            return joined(&parts, &units(",")).map(Some);
        }

        if let Some(template) = node.template() {
            let mut value = quasi(template, 0);

            for (index, expression) in template.expressions.iter().enumerate() {
                let part = self.text(
                    Node::Expression(expression),
                    &mut seen.clone(),
                    budget,
                    depth + 1,
                )?;
                value = concatenate(&[
                    &value,
                    &part.unwrap_or_else(|| units(INTERPOLATION)),
                    &quasi(template, index + 1),
                ])?;
            }

            return Ok(Some(value));
        }

        if let Some([left, right]) = node.addition() {
            let left = self.text(left, &mut seen.clone(), budget, depth + 1)?;
            let right = self.text(right, &mut seen.clone(), budget, depth + 1)?;

            return match (left, right) {
                (Some(left), Some(right)) => concatenate(&[&left, &right]).map(Some),
                _ => Ok(None),
            };
        }

        if let Some((call, name, receiver)) = Self::method_call(node) {
            if equals(&name, "concat") {
                let receiver = self.text(receiver, &mut seen.clone(), budget, depth + 1)?;
                let Some(arguments) =
                    self.call_arguments(&call.arguments, seen, budget, depth + 1)?
                else {
                    return Ok(None);
                };
                let mut parts = Vec::new();

                for argument in arguments {
                    parts.push(self.text(argument, &mut seen.clone(), budget, depth + 1)?);
                }

                let (Some(receiver), Some(parts)) =
                    (receiver, parts.into_iter().collect::<Option<Vec<Text>>>())
                else {
                    return Ok(None);
                };
                let mut all: Vec<&[u16]> = vec![&receiver];
                all.extend(parts.iter().map(Vec::as_slice));

                return concatenate(&all).map(Some);
            }

            if (equals(&name, "replace") || equals(&name, "replaceAll"))
                && call.arguments.len() == 2
            {
                let receiver = self.text(receiver, &mut seen.clone(), budget, depth + 1)?;
                let search = self.text(
                    argument_node(&call.arguments[0]),
                    &mut seen.clone(),
                    budget,
                    depth + 1,
                )?;
                let replacement = self.text(
                    argument_node(&call.arguments[1]),
                    &mut seen.clone(),
                    budget,
                    depth + 1,
                )?;
                // A search or replacement that is not rebuilt, such as a regular expression or a
                // callback, fails closed (see the module documentation).
                if receiver.is_some() && (search.is_none() || replacement.is_none()) {
                    self.unrebuilt.set(true);
                }

                let (Some(receiver), Some(search), Some(replacement)) =
                    (receiver, search, replacement)
                else {
                    return Ok(None);
                };
                let all = equals(&name, "replaceAll");
                validate_replacement(&receiver, &replacement, all)?;

                return validate(replace(&receiver, &search, &replacement, all)).map(Some);
            }

            if equals(&name, "join") {
                let Some(elements) =
                    self.array_elements(receiver, &mut seen.clone(), budget, depth + 1)?
                else {
                    return Ok(None);
                };
                let delimiter = match call.arguments.first() {
                    None => Some(units(",")),
                    Some(argument) => self.text(
                        argument_node(argument),
                        &mut seen.clone(),
                        budget,
                        depth + 1,
                    )?,
                };
                let mut parts = Vec::new();

                for element in elements {
                    parts.push(self.text(element, &mut seen.clone(), budget, depth + 1)?);
                }

                let (Some(delimiter), Some(parts)) =
                    (delimiter, parts.into_iter().collect::<Option<Vec<Text>>>())
                else {
                    return Ok(None);
                };

                return joined(&parts, &delimiter).map(Some);
            }

            // A method the reconstruction does not model, called on a text it rebuilt, fails
            // closed (see the module documentation).
            if self
                .text(receiver, &mut seen.clone(), budget, depth + 1)?
                .is_some()
            {
                self.unrebuilt.set(true);
            }
        }

        if let [initializer] = self.property_initializers(node).as_slice() {
            return self.text(*initializer, &mut seen.clone(), budget, depth + 1);
        }

        if let Some(name) = node.identifier().filter(|name| !seen.contains(name))
            && let Some(initializer) = self.unique(name)
        {
            seen.insert(name);

            return self.text(initializer, seen, budget, depth + 1);
        }

        Ok(None)
    }

    /// Every text a node may evaluate to, following every assignment and branch.
    #[expect(
        clippy::too_many_lines,
        reason = "one candidate reconstruction keeps the order of its branches"
    )]
    pub(super) fn candidates(
        &self,
        node: Node<'a>,
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Vec<Text>> {
        consume(budget, depth)?;

        if let Some((hops, text)) = self.atomic(node, seen, depth)? {
            Self::consume_atomic(hops, budget, depth)?;

            return Ok(vec![text]);
        }

        if let Some(inner) = node.typed_operand() {
            return self.candidates(inner, seen, budget, depth + 1);
        }

        if let Some(branches) = node.branches() {
            let mut candidates = Candidates::default();

            for branch in branches {
                for candidate in self.candidates(branch, seen, budget, depth + 1)? {
                    candidates.add(candidate, budget, depth)?;
                }
            }

            return Ok(candidates.0);
        }

        if let Node::Expression(Expression::TaggedTemplateExpression(tagged)) = node {
            return self.candidates(Node::Template(&tagged.quasi), seen, budget, depth + 1);
        }

        if let Some(name) = node.identifier() {
            if seen.contains(name) {
                return Ok(Vec::new());
            }

            let mut next = seen.clone();
            next.insert(name);
            let mut candidates = Candidates::default();

            for initializer in self.get(name) {
                for candidate in self.candidates(*initializer, &next, budget, depth + 1)? {
                    candidates.add(candidate, budget, depth + 1)?;
                }
            }

            return Ok(candidates.0);
        }

        if let Some([left, right]) = node.addition() {
            let left = self.candidates(left, seen, budget, depth + 1)?;
            let right = self.candidates(right, seen, budget, depth + 1)?;

            return concatenate_candidates(&left, &right, budget, depth + 1);
        }

        if let Some(template) = node.template() {
            let mut candidates = vec![validate(quasi(template, 0))?];

            for (index, expression) in template.expressions.iter().enumerate() {
                let parts =
                    self.candidates(Node::Expression(expression), seen, budget, depth + 1)?;
                let parts = if parts.is_empty() {
                    vec![units(INTERPOLATION)]
                } else {
                    parts
                };
                let tail = quasi(template, index + 1);
                candidates = concatenate_candidates(&candidates, &parts, budget, depth + 1)?
                    .iter()
                    .map(|candidate| concatenate(&[candidate, &tail]))
                    .collect::<Outcome<Vec<Text>>>()?;
            }

            return Ok(candidates);
        }

        let method = Self::method_call(node);

        if let Some((call, name, receiver)) = &method
            && equals(name, "concat")
        {
            let mut candidates = self.candidates(*receiver, seen, budget, depth + 1)?;
            let Some(arguments) = self.call_arguments(&call.arguments, seen, budget, depth + 1)?
            else {
                return Ok(Vec::new());
            };

            for argument in arguments {
                let parts = self.candidates(argument, seen, budget, depth + 1)?;
                candidates = concatenate_candidates(&candidates, &parts, budget, depth + 1)?;

                if candidates.is_empty() {
                    return Ok(Vec::new());
                }
            }

            return Ok(candidates);
        }

        if let Some((call, name, receiver)) = &method
            && equals(name, "join")
        {
            let arrays = self.array_element_candidates(*receiver, seen, budget, depth + 1)?;
            let delimiters = match call.arguments.first() {
                None => vec![units(",")],
                Some(argument) => {
                    self.candidates(argument_node(argument), seen, budget, depth + 1)?
                }
            };
            let mut candidates = Candidates::default();

            for elements in arrays {
                let mut joined = vec![Text::new()];

                for (index, element) in elements.into_iter().enumerate() {
                    if index > 0 {
                        joined = concatenate_candidates(&joined, &delimiters, budget, depth + 1)?;
                    }

                    let parts = self.candidates(element, seen, budget, depth + 1)?;
                    joined = concatenate_candidates(&joined, &parts, budget, depth + 1)?;
                }

                for candidate in joined {
                    candidates.add(candidate, budget, depth + 1)?;
                }
            }

            return Ok(candidates.0);
        }

        if let Some((call, name, receiver)) = &method
            && (equals(name, "replace") || equals(name, "replaceAll"))
            && call.arguments.len() == 2
        {
            let receivers = self.candidates(*receiver, seen, budget, depth + 1)?;
            let replacements =
                self.candidates(argument_node(&call.arguments[1]), seen, budget, depth + 1)?;
            let searches =
                self.candidates(argument_node(&call.arguments[0]), seen, budget, depth + 1)?;

            // A search or replacement that is not rebuilt, such as a regular expression or a
            // callback, fails closed (see the module documentation).
            if !receivers.is_empty() && (searches.is_empty() || replacements.is_empty()) {
                self.unrebuilt.set(true);
            }

            let all = equals(name, "replaceAll");
            let mut candidates = Candidates::default();

            for receiver in &receivers {
                for search in &searches {
                    for replacement in &replacements {
                        validate_replacement(receiver, replacement, all)?;
                        candidates.add(
                            replace(receiver, search, replacement, all),
                            budget,
                            depth + 1,
                        )?;
                    }
                }
            }

            return Ok(candidates.0);
        }

        let initializers = self.property_initializers(node);

        if !initializers.is_empty() {
            let mut candidates = Candidates::default();

            for initializer in initializers {
                for candidate in self.candidates(initializer, seen, budget, depth + 1)? {
                    candidates.add(candidate, budget, depth + 1)?;
                }
            }

            return Ok(candidates.0);
        }

        Ok(self
            .text(node, &mut seen.clone(), budget, depth + 1)?
            .map_or_else(Vec::new, |text| vec![text]))
    }

    pub(super) fn fresh_text(&self, node: Node<'a>) -> Outcome<Option<Text>> {
        self.text(node, &mut Seen::default(), &mut 0, 0)
    }

    pub(super) fn fresh_candidates(&self, node: Node<'a>) -> Outcome<Vec<Text>> {
        self.candidates(node, &Seen::default(), &mut 0, 0)
    }
}

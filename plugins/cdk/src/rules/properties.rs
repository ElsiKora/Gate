//! `cdk/properties`: a stack's, stage's or construct's constructor is `(scope, id, properties)`,
//! the third typed `I<Class>Properties` from `interface/` of its own directory; no interface is
//! named `…Props`.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Class, ClassElement, MethodDefinitionKind, TSType, TSTypeName,
};
use gate_typescript::{SourceFile, TypeScript};

use crate::options::PluginOptions;
use crate::rules::{Found, folder, import_source, report};
use crate::sources::{Cdk, code_files, parsed};

/// The rule.
#[derive(Debug, Default)]
pub struct Properties;

impl Rule<Cdk> for Properties {
    type Options = NoOptions;

    const NAME: &'static str = "properties";
    const TITLE: &'static str = "A construct takes (scope, id, properties), typed I<Class>Properties from its interface/ folder";
    const ASPECTS: &'static [&'static str] = &["parameter", "interface"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            for path in code_files(typescript, "lib/") {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                for found in judge(typescript, path, &file)? {
                    report(run, found);
                }
            }
            Ok(())
        })
    }
}

fn judge<'p>(
    typescript: &TypeScript<'_>,
    path: &'p str,
    file: &SourceFile<'_>,
) -> gate_sdk::Result<Vec<Found<'p>>> {
    let mut found = Vec::new();
    let library = ImportBindings::new(file, "aws-cdk-lib", false);
    let constructs = ImportBindings::new(file, "constructs", false);

    for node in file.semantic().nodes().iter() {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(interface)
                if interface.id.name.as_str().ends_with("Props") =>
            {
                let name = interface.id.name.as_str();
                found.push(Found {
                    aspect: "interface",
                    path,
                    line: Some(file.line(interface.span.start)),
                    key: name.to_owned(),
                    message: format!("{path} declares {name}; the word is Properties, not Props"),
                    help: "name the interface I<Class>Properties",
                });
            }
            AstKind::Class(class) => {
                let Some(heritage) = class.heritage.as_ref() else {
                    continue;
                };
                let base = library
                    .as_ref()
                    .and_then(|bindings| bindings.resolve(&heritage.expression))
                    .or_else(|| {
                        constructs
                            .as_ref()
                            .and_then(|bindings| bindings.resolve(&heritage.expression))
                    });
                let is_construct = base.as_deref().is_some_and(|base| {
                    matches!(base, [name] if name == "Stack" || name == "Stage" || name == "Construct")
                });
                if is_construct {
                    constructor(typescript, path, file, class, &mut found)?;
                }
            }
            _ => {}
        }
    }

    Ok(found)
}

fn constructor<'p>(
    typescript: &TypeScript<'_>,
    path: &'p str,
    file: &SourceFile<'_>,
    class: &Class<'_>,
    found: &mut Vec<Found<'p>>,
) -> gate_sdk::Result<()> {
    let Some(class_name) = class.id.as_ref().map(|id| id.name.as_str()) else {
        return Ok(());
    };
    let Some(method) = class.body.body.iter().find_map(|element| match element {
        ClassElement::MethodDefinition(method)
            if method.kind == MethodDefinitionKind::Constructor =>
        {
            Some(method)
        }
        _ => None,
    }) else {
        return Ok(());
    };
    let Some(parameter) = method.value.params.items.get(2) else {
        return Ok(());
    };
    let line = Some(file.line(parameter.span.start));

    let binding_name = match &parameter.pattern {
        BindingPattern::BindingIdentifier(binding) => binding.name.as_str(),
        _ => "",
    };
    if binding_name != "properties" {
        found.push(Found {
            aspect: "parameter",
            path,
            line,
            key: class_name.to_owned(),
            message: format!(
                "{path}: the third parameter of {class_name}'s constructor is not named properties"
            ),
            help: "name the parameter properties",
        });
    }

    let wanted = format!("I{class_name}Properties");
    let typed = parameter.type_annotation.as_ref().and_then(|annotation| {
        match &annotation.type_annotation {
            TSType::TSTypeReference(reference) => match &reference.type_name {
                TSTypeName::IdentifierReference(name) => Some(name.name.as_str()),
                _ => None,
            },
            _ => None,
        }
    });
    let directory = folder(path);
    let declared = match typed {
        Some(name) if name == wanted => match import_source(file, name) {
            Some(specifier) => typescript.resolve(path, specifier)?.is_some_and(|target| {
                target == format!("{directory}/interface/properties.interface.ts")
                    || target == format!("{directory}/interface/index.ts")
            }),
            None => false,
        },
        _ => false,
    };
    if !declared {
        found.push(Found {
            aspect: "interface",
            path,
            line,
            key: class_name.to_owned(),
            message: format!(
                "{path}: {class_name}'s properties are not typed {wanted} from {directory}/interface/properties.interface.ts"
            ),
            help: "declare I<Class>Properties in interface/properties.interface.ts beside the class and type the parameter with it",
        });
    }

    Ok(())
}

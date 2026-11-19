//! `nestjs/runtime-factory`: each runtime role's start file constructs its Nest application only
//! through the factory methods of its role contract.

use crate::shared::paths::{self, Form};
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::ast::CallExpression;
use gate_typescript::oxc_ast_visit::{Visit, walk};

use crate::layout::PluginOptions;
use crate::shared::project;

/// The rule.
#[derive(Debug, Default)]
pub struct RuntimeFactory;

/// One start file and the factory methods it calls.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Contract {
    /// The start file, relative to the project root.
    pub file: String,
    /// The factory methods it must call, and the only ones it may call.
    pub methods: Vec<String>,
}

/// The options of `nestjs/runtime-factory`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The start files and their methods; at least one.
    #[schemars(length(min = 1))]
    pub contracts: Vec<Contract>,
    /// The factory class of the `layout.nest-core-package` package.
    #[serde(default = "factory_class")]
    pub factory_class: String,
}

/// The default `factory-class`.
fn factory_class() -> String {
    "NestFactory".to_owned()
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if self.contracts.is_empty() {
            return Err(gate_sdk::Error::config(
                "option `contracts` lists nothing; list at least one start file, or leave the rule off".to_owned(),
            ));
        }
        for (index, contract) in self.contracts.iter().enumerate() {
            paths::check(
                &format!("contracts[{index}].file"),
                &contract.file,
                Form::File,
            )?;
        }
        Ok(())
    }
}

impl Rule<crate::Nest> for RuntimeFactory {
    type Options = Options;

    const NAME: &'static str = "runtime-factory";
    const TITLE: &'static str =
        "Runtime start files construct Nest only through their role's NestFactory methods";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            run.options().check_paths()?;
            let contracts = run.options().contracts.clone();
            let class = run.options().factory_class.clone();
            let typescript = shared.typescript()?;

            for contract in &contracts {
                let path = contract.file.as_str();

                if !run.tree().contains(path) {
                    run.report()
                        .finding(
                            "",
                            Location::file(path),
                            format!("{path} is required for the native {class} role contract"),
                        )
                        .help("restore the start file or remove its contract")
                        .key("required");
                    continue;
                }

                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };
                let calls = factory_calls(&file, &layout.nest_core_package, &class);
                let mut listed: Vec<&str> = Vec::new();

                for method in &contract.methods {
                    if listed.contains(&method.as_str()) {
                        continue;
                    }
                    listed.push(method);

                    if !calls.iter().any(|(called, _)| called == method) {
                        run.report()
                            .finding(
                                "",
                                Location::file(path),
                                format!("{path} must call native {class}.{method}"),
                            )
                            .help(format!(
                                "construct the application with {class}.{method}, as the role's \
                             contract says"
                            ))
                            .key(format!("{method}#missing"));
                    }
                }

                for (method, line) in &calls {
                    if !contract.methods.contains(method) {
                        run.report()
                        .finding(
                            "",
                            Location::line(path, usize::try_from(*line).unwrap_or(usize::MAX)),
                            format!(
                                "{path} calls native {class}.{method} outside its role contract"
                            ),
                        )
                        .help(format!(
                            "use only the role's {class} methods, or add {method} to its contract"
                        ))
                        .key(format!("{method}#unlisted"));
                    }
                }
            }

            Ok(())
        })
    }
}

/// Every method of `class` from `package` the file calls, with the line of its first call in
/// source order.
fn factory_calls(file: &SourceFile<'_>, package: &str, class: &str) -> Vec<(String, u32)> {
    let Some(bindings) = ImportBindings::new(file, package, false) else {
        return Vec::new();
    };
    let mut finder = CallFinder {
        file,
        bindings: &bindings,
        class,
        calls: Vec::new(),
    };
    finder.visit_program(file.program());
    finder.calls
}

/// Collects the factory calls of a file.
struct CallFinder<'f, 'b, 's, 'a> {
    file: &'f SourceFile<'a>,
    bindings: &'b ImportBindings<'s, 'a>,
    class: &'f str,
    calls: Vec<(String, u32)>,
}

impl<'a> Visit<'a> for CallFinder<'_, '_, '_, 'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some([class, method]) = self.bindings.resolve(&call.callee).as_deref()
            && class == self.class
            && !self.calls.iter().any(|(called, _)| called == method)
        {
            self.calls
                .push((method.clone(), self.file.line(call.span.start)));
        }

        walk::walk_call_expression(self, call);
    }
}

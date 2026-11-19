//! `nestjs/asset-storage`: business modules own no public-asset upload or storage path, persisted
//! image references are keys, the S3 SDK stays in infrastructure, the CDN SDK stays out of runtime
//! code, and the S3 adapter keeps the public-asset S3 conventions once central admission exists.

mod tokens;

use crate::shared::paths::{self, Form};
use std::collections::BTreeSet;
use std::rc::Rc;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::oxc_ast::ast::{
    BindingPattern, ClassElement, Declaration, Decorator, Expression, ImportDeclarationSpecifier,
    PropertyKey, Statement, VariableDeclaration, VariableDeclarationKind,
};
use gate_typescript::oxc_ast_visit::Visit;
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::{Allocator, Source, SourceFile, TypeScript};
use regex_automata::meta::Regex;

use self::tokens::Tokens;
use crate::layout::{Layout, PluginOptions, folder_prefix};
use crate::shared::report::Findings;
use crate::shared::syntax::top_level_classes;
use crate::shared::{pattern, project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct AssetStorage;

/// The options of `nestjs/asset-storage`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The file whose presence marks that central admission exists; by default
    /// `internal/public-asset/controller.ts` below the modules root.
    pub central_anchor: Option<String>,
    /// The central admission resource of the internal plane, exempt from `central-admission`; by
    /// default `internal/public-asset/` below the modules root.
    pub central_internal_folder: Option<String>,
    /// The central resource of the domain plane, exempt from `central-admission`; by default
    /// `<domain-folder>/public-asset/` below the modules root.
    pub central_domain_folder: Option<String>,
    /// The S3 adapter; by default `public-asset/s3/adapter.ts` below the infrastructure folder.
    pub s3_adapter: Option<String>,
    /// The file that owns the public-asset S3 policy constants; by default
    /// `public-asset/constant/s3.constant.ts` below the infrastructure folder.
    pub s3_policy_constant: Option<String>,
    /// The S3 SDK package.
    pub s3_package: String,
    /// The CDN SDK package.
    pub cloudfront_package: String,
    /// Names of persisted fields that hold an image.
    pub image_name_pattern: String,
    /// The ending of a persisted image reference's name.
    pub image_key_suffix: String,
    /// String literals that are image routes.
    pub image_route_pattern: String,
    /// Names of asset commands.
    pub asset_command_pattern: String,
    /// Names whose mention in a domain file consumes public-asset storage.
    pub storage_access_names: Vec<String>,
    /// Names that admit public-asset bytes.
    pub admission_names: Vec<String>,
    /// Names of local HTTP status aliases in the adapter.
    pub http_status_alias_pattern: String,
    /// Enum members the adapter must use, as `[enum, member]`.
    pub required_members: Vec<[String; 2]>,
    /// Properties the adapter must not set to the literal member name, as `[key, value]`.
    pub forbidden_literals: Vec<[String; 2]>,
}

impl Default for Options {
    fn default() -> Self {
        let pairs = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(left, right)| [(*left).to_owned(), (*right).to_owned()])
                .collect()
        };

        Self {
            central_anchor: None,
            central_internal_folder: None,
            central_domain_folder: None,
            s3_adapter: None,
            s3_policy_constant: None,
            s3_package: "@aws-sdk/client-s3".to_owned(),
            cloudfront_package: "@aws-sdk/client-cloudfront".to_owned(),
            image_name_pattern: "(?i)(?:avatar|banner|icon|image|logo|photo|picture|thumbnail)"
                .to_owned(),
            image_key_suffix: "Key$".to_owned(),
            image_route_pattern: "(?:^|/)[:A-Za-z0-9_-]*image(?:/|$)".to_owned(),
            asset_command_pattern:
                "^(?:createWith|replace|set|clear)[A-Za-z0-9_]*(?:Asset|Image)[A-Za-z0-9_]*$"
                    .to_owned(),
            storage_access_names: ["IPublicAssetStorage", "PUBLIC_ASSET_STORAGE_TOKEN_CONSTANT"]
                .map(str::to_owned)
                .to_vec(),
            admission_names: ["contentBase64", "declaredMediaType"]
                .map(str::to_owned)
                .to_vec(),
            http_status_alias_pattern: "^HTTP_STATUS_[A-Z0-9_]+$".to_owned(),
            required_members: pairs(&[
                ("HttpStatus", "CONFLICT"),
                ("HttpStatus", "NOT_FOUND"),
                ("HttpStatus", "PRECONDITION_FAILED"),
                ("ChecksumAlgorithm", "SHA256"),
                ("ChecksumMode", "ENABLED"),
            ]),
            forbidden_literals: pairs(&[
                ("ChecksumAlgorithm", "SHA256"),
                ("ChecksumMode", "ENABLED"),
            ]),
        }
    }
}

/// The central public-asset paths, as set or derived from the layout.
struct Central {
    anchor: String,
    internal_folder: String,
    domain_folder: String,
    s3_adapter: String,
    s3_policy_constant: String,
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        for (option, value, form) in [
            ("central-anchor", &self.central_anchor, Form::File),
            (
                "central-internal-folder",
                &self.central_internal_folder,
                Form::Prefix,
            ),
            (
                "central-domain-folder",
                &self.central_domain_folder,
                Form::Prefix,
            ),
            ("s3-adapter", &self.s3_adapter, Form::File),
            ("s3-policy-constant", &self.s3_policy_constant, Form::File),
        ] {
            if let Some(value) = value {
                paths::check(option, value, form)?;
            }
        }
        Ok(())
    }

    /// The central paths: each option, or its default below the layout's folders.
    fn central(&self, layout: &Layout) -> Central {
        let modules = folder_prefix(&layout.modules_root);
        let infrastructure = folder_prefix(&layout.infrastructure_folder);
        let or = |value: &Option<String>, default: String| value.clone().unwrap_or(default);

        Central {
            anchor: or(
                &self.central_anchor,
                format!("{modules}internal/public-asset/controller.ts"),
            ),
            internal_folder: or(
                &self.central_internal_folder,
                format!("{modules}internal/public-asset/"),
            ),
            domain_folder: or(
                &self.central_domain_folder,
                format!("{modules}{}/public-asset/", layout.domain_folder),
            ),
            s3_adapter: or(
                &self.s3_adapter,
                format!("{infrastructure}public-asset/s3/adapter.ts"),
            ),
            s3_policy_constant: or(
                &self.s3_policy_constant,
                format!("{infrastructure}public-asset/constant/s3.constant.ts"),
            ),
        }
    }
}

impl Rule for AssetStorage {
    type Options = Options;

    const NAME: &'static str = "asset-storage";
    const TITLE: &'static str =
        "Public assets are admitted centrally and stored only through the S3 adapter";
    const ASPECTS: &'static [&'static str] = &[
        "central-admission",
        "cloudfront-cdk-only",
        "domain-storage-access",
        "persisted-image-key",
        "s3-adapter-required",
        "s3-policy-constant",
        "s3-sdk-enum",
        "s3-sdk-placement",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        run.options().check_paths()?;
        let options = run.options();
        let patterns = Patterns::new(options)?;
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let central = options.central(&layout);
        let assets = Assets {
            options,
            central: &central,
            layout: &layout,
            patterns: &patterns,
            source: &source,
            typescript: &typescript,
        };
        let mut findings = Findings::default();

        assets.persisted_image_keys(&mut findings)?;
        assets.external_sdk_placement(&mut findings)?;

        if source.contains(&central.anchor) {
            assets.s3_adapter_conventions(&mut findings)?;
            assets.central_admission_placement(&mut findings)?;
        }

        findings.report(run, help);
        Ok(())
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "persisted-image-key" => "persist the asset's key, with a name ending in the key suffix",
        "cloudfront-cdk-only" => "manage the CDN in the infrastructure-as-code project",
        "s3-sdk-placement" => "use the S3 SDK only through the infrastructure adapter",
        "s3-adapter-required" => "add the S3 adapter of the public-asset infrastructure",
        "s3-policy-constant" => "keep the S3 policy constants in their constant file",
        "s3-sdk-enum" => "use the SDK's and the framework's enum members",
        "domain-storage-access" => "reach public-asset storage through the central resource",
        _ => "admit and store assets only through the central public-asset resource",
    }
}

/// The compiled patterns of the options.
struct Patterns {
    image_like: Regex,
    key_suffix: Regex,
    upper_case: Regex,
    http_status_alias: Regex,
    image_route: Regex,
    asset_command: Regex,
    storage_access: Option<Regex>,
}

impl Patterns {
    fn new(options: &Options) -> gate_sdk::Result<Self> {
        let names: Vec<String> = options
            .storage_access_names
            .iter()
            .map(|name| pattern::escape(name))
            .collect();

        Ok(Self {
            image_like: pattern::compile("image-name-pattern", &options.image_name_pattern)?,
            key_suffix: pattern::compile("image-key-suffix", &options.image_key_suffix)?,
            upper_case: pattern::compile("upper-case", "^[A-Z][A-Z0-9_]*$")?,
            http_status_alias: pattern::compile(
                "http-status-alias-pattern",
                &options.http_status_alias_pattern,
            )?,
            image_route: pattern::compile("image-route-pattern", &options.image_route_pattern)?,
            asset_command: pattern::compile(
                "asset-command-pattern",
                &options.asset_command_pattern,
            )?,
            storage_access: if names.is_empty() {
                None
            } else {
                Some(pattern::compile(
                    "storage-access-names",
                    &format!("(?-u:\\b)(?:{})(?-u:\\b)", names.join("|")),
                )?)
            },
        })
    }
}

/// The run's view of the tree, its settings and its options.
struct Assets<'r, 'a> {
    options: &'r Options,
    central: &'r Central,
    layout: &'r Layout,
    patterns: &'r Patterns,
    source: &'r dyn Source,
    typescript: &'r TypeScript<'a>,
}

impl<'a> Assets<'_, 'a> {
    /// The files with the `.ts` extension below `directory`, compared case-sensitively, in every
    /// folder: the rule skips none ([`tree::UNSKIPPED`]).
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "the extension is compared case-sensitively, as the rule documents"
    )]
    fn under(&self, directory: &str) -> Vec<&'a str> {
        tree::files(self.typescript.source(), directory, &tree::UNSKIPPED)
            .filter(|path| path.ends_with(".ts"))
            .collect()
    }

    fn tokens(&self, path: &str) -> gate_sdk::Result<Option<(Rc<SourceFile<'a>>, Tokens)>> {
        Ok(project::parsed(self.typescript, path)?.map(|file| {
            let mut tokens = Tokens::default();
            tokens.visit_program(file.program());
            (file, tokens)
        }))
    }

    /// The domain plane of the modules.
    fn domain(&self) -> String {
        format!(
            "{}{}",
            folder_prefix(&self.layout.modules_root),
            self.layout.domain_folder
        )
    }

    fn persisted_image_keys(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        for path in self.under(&self.domain()) {
            let Some(file) = project::parsed(self.typescript, path)? else {
                continue;
            };
            let bindings = TypeOrmBindings::read(&file.program().body, self.layout);

            for class in top_level_classes(&file.program().body) {
                for element in &class.body.body {
                    let (decorators, key, computed) = match element {
                        ClassElement::PropertyDefinition(property) => {
                            (&property.decorators, &property.key, property.computed)
                        }
                        ClassElement::AccessorProperty(property) => {
                            (&property.decorators, &property.key, property.computed)
                        }
                        _ => continue,
                    };

                    if !decorators
                        .iter()
                        .any(|decorator| bindings.persists(decorator, self.layout))
                    {
                        continue;
                    }

                    let name = key_text(file.source(), element.span(), key, computed)
                        .replace(['"', '\''], "");

                    if self.patterns.image_like.is_match(name.as_str())
                        && !self.patterns.key_suffix.is_match(name.as_str())
                    {
                        let start = decorators
                            .iter()
                            .map(|decorator| decorator.span.start)
                            .chain([element.span().start])
                            .min()
                            .unwrap_or_default();
                        findings.push(
                            "persisted-image-key",
                            path,
                            Some(file.line(start)),
                            format!(
                                "{path} persists image-like field {name}; persisted image \
                                 references must end in Key"
                            ),
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn external_sdk_placement(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        let loads = |tokens: &Tokens, package: &str| {
            tokens
                .modules
                .iter()
                .any(|module| module == package || module.starts_with(&format!("{package}/")))
        };
        let infrastructure = folder_prefix(&self.layout.infrastructure_folder);

        for path in self.under(&self.layout.source_root) {
            let Some((_, tokens)) = self.tokens(path)? else {
                continue;
            };

            if loads(&tokens, &self.options.cloudfront_package) {
                findings.push(
                    "cloudfront-cdk-only",
                    path,
                    None,
                    format!("{path} imports CloudFront at runtime; CloudFront is CDK-only"),
                );
            }

            if !path.starts_with(&infrastructure) && loads(&tokens, &self.options.s3_package) {
                findings.push(
                    "s3-sdk-placement",
                    path,
                    None,
                    format!("{path} imports the S3 SDK outside infrastructure"),
                );
            }
        }

        Ok(())
    }

    fn s3_adapter_conventions(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        let adapter = self.central.s3_adapter.as_str();
        let constant = self.central.s3_policy_constant.as_str();
        let upper_case = &self.patterns.upper_case;

        if !self.source.contains(adapter) {
            findings.push(
                "s3-adapter-required",
                adapter,
                None,
                format!("{adapter} is required once central PublicAsset admission exists"),
            );
            return Ok(());
        }

        // A constant file without a syntax tree is reported by the parse rule, not as missing.
        match project::parsed(self.typescript, constant)? {
            None if !self.source.contains(constant) => findings.push(
                "s3-policy-constant",
                constant,
                None,
                format!("{constant} must own public-asset S3 policy constants"),
            ),
            Some(file)
                if !top_level_const_names(&file.program().body)
                    .iter()
                    .any(|name| upper_case.is_match(name.as_str())) =>
            {
                findings.push(
                    "s3-policy-constant",
                    constant,
                    None,
                    format!("{constant} must declare the owned UPPER_CASE S3 policy constant bag"),
                );
            }
            _ => {}
        }

        let Some((file, tokens)) = self.tokens(adapter)? else {
            return Ok(());
        };

        for name in top_level_const_names(&file.program().body) {
            if upper_case.is_match(name.as_str()) {
                findings.push(
                    "s3-policy-constant",
                    adapter,
                    None,
                    format!(
                        "{adapter} declares top-level UPPER_CASE const {name}; public-asset S3 \
                         policy constants belong in {constant}"
                    ),
                );
            }
        }

        for assignment in &tokens.assignments {
            if let Some([key, value]) = self
                .options
                .forbidden_literals
                .iter()
                .find(|[key, value]| *key == assignment.key && *value == assignment.value)
            {
                findings.push(
                    "s3-sdk-enum",
                    adapter,
                    Some(file.line(assignment.offset)),
                    format!("{adapter} uses literal {key}: \"{value}\"; use {key}.{value}"),
                );
            }
        }

        let aliases: BTreeSet<&String> = tokens
            .identifiers
            .iter()
            .filter(|name| self.patterns.http_status_alias.is_match(name.as_str()))
            .collect();

        for alias in aliases {
            findings.push(
                "s3-sdk-enum",
                adapter,
                None,
                format!("{adapter} declares {alias}; use HttpStatus enum members"),
            );
        }

        for [object, property] in &self.options.required_members {
            if !tokens.members.contains(&(object.clone(), property.clone())) {
                findings.push(
                    "s3-sdk-enum",
                    adapter,
                    None,
                    format!("{adapter} must use {object}.{property}"),
                );
            }
        }

        Ok(())
    }

    fn central_admission_placement(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        let options = self.options;
        let asset_command =
            |tokens: &Tokens| tokens.any_name(|name| self.patterns.asset_command.is_match(name));

        for plane in self.layout.planes_without_domain() {
            for path in self.under(&format!(
                "{}{plane}",
                folder_prefix(&self.layout.modules_root)
            )) {
                if path.starts_with(self.central.internal_folder.as_str()) {
                    continue;
                }

                let Some((_, tokens)) = self.tokens(path)? else {
                    continue;
                };

                if tokens.any_name(|name| options.admission_names.iter().any(|known| known == name))
                {
                    findings.push(
                        "central-admission",
                        path,
                        None,
                        format!(
                            "{path} admits public-asset bytes outside the central internal \
                             PublicAsset resource"
                        ),
                    );
                }

                if tokens
                    .strings
                    .iter()
                    .any(|text| self.patterns.image_route.is_match(text.as_str()))
                {
                    findings.push(
                        "central-admission",
                        path,
                        None,
                        format!(
                            "{path} declares a feature image route outside central PublicAsset \
                             CREATE"
                        ),
                    );
                }

                if asset_command(&tokens) {
                    findings.push(
                        "central-admission",
                        path,
                        None,
                        format!(
                            "{path} declares a feature asset command outside central PublicAsset \
                             CREATE"
                        ),
                    );
                }
            }
        }

        for path in self.under(&self.domain()) {
            if path.starts_with(self.central.domain_folder.as_str()) {
                continue;
            }

            if let Some((_, tokens)) = self.tokens(path)?
                && asset_command(&tokens)
            {
                findings.push(
                    "central-admission",
                    path,
                    None,
                    format!("{path} declares a feature asset command in a business domain"),
                );
            }

            // The storage search reads the text lossily, so it needs no parse and a file that is
            // not UTF-8 text does not stop the run.
            if let Some(storage) = &self.patterns.storage_access
                && self
                    .source
                    .bytes(path)
                    .map_err(|failure| project::error(&failure))?
                    .is_some_and(|bytes| storage.is_match(String::from_utf8_lossy(&bytes).as_ref()))
            {
                findings.push(
                    "domain-storage-access",
                    path,
                    None,
                    format!("{path} consumes public-asset storage from a business domain"),
                );
            }
        }

        Ok(())
    }
}

/// The local names under which a file imports persisted-column decorators from the ORM package.
#[derive(Debug, Default)]
struct TypeOrmBindings {
    /// Local name to imported decorator name.
    named: Vec<(String, String)>,
    namespaces: Vec<String>,
}

impl TypeOrmBindings {
    fn read(body: &[Statement<'_>], layout: &Layout) -> Self {
        let mut bindings = Self::default();

        for statement in body {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };

            if import.source.value != layout.typeorm_package.as_str() {
                continue;
            }

            for specifier in import.specifiers.iter().flatten() {
                match specifier {
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                        bindings.namespaces.push(namespace.local.name.to_string());
                    }
                    ImportDeclarationSpecifier::ImportSpecifier(named) => {
                        let imported = named.imported.name().to_string();

                        if layout.typeorm_column_decorators.contains(&imported) {
                            bindings
                                .named
                                .push((named.local.name.to_string(), imported));
                        }
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
                }
            }
        }

        bindings
    }

    /// Whether a decorator, or the callee of a decorator call, names a persisted-column
    /// decorator imported from the ORM package.
    fn persists(&self, decorator: &Decorator<'_>, layout: &Layout) -> bool {
        let expression = match &decorator.expression {
            Expression::CallExpression(call) => &call.callee,
            other => other,
        };
        let name = match expression {
            Expression::Identifier(identifier) => self
                .named
                .iter()
                .rev()
                .find(|(local, _)| local == identifier.name.as_str())
                .map(|(_, imported)| imported.as_str()),
            Expression::StaticMemberExpression(member) => match &member.object {
                Expression::Identifier(object)
                    if self
                        .namespaces
                        .iter()
                        .any(|namespace| namespace == object.name.as_str()) =>
                {
                    Some(member.property.name.as_str())
                }
                _ => None,
            },
            _ => None,
        };

        name.is_some_and(|name| {
            layout
                .typeorm_column_decorators
                .iter()
                .any(|known| known == name)
        })
    }
}

/// The source text of a property name as the compiler's `getText` returns it: a computed name
/// keeps its brackets and anything between them.
fn key_text(source: &str, element: Span, key: &PropertyKey<'_>, computed: bool) -> String {
    let span = key.span();
    let (mut start, mut end) = (span.start as usize, span.end as usize);

    if computed {
        if let Some(open) = source
            .get(element.start as usize..start)
            .and_then(|prefix| prefix.rfind('['))
        {
            start = element.start as usize + open;
        }

        if let Some(close) = source
            .get(end..element.end as usize)
            .and_then(|suffix| suffix.find(']'))
        {
            end += close + 1;
        }
    }

    source.get(start..end).unwrap_or_default().to_owned()
}

/// The names bound by the top-level `const` declarations of a file, exported or not.
fn top_level_const_names(body: &[Statement<'_>]) -> Vec<String> {
    let mut names = Vec::new();

    for statement in body {
        let declaration: Option<&VariableDeclaration<'_>> = match statement {
            Statement::VariableDeclaration(declaration) => Some(&**declaration),
            Statement::ExportDeclaration(export) => match &export.declaration {
                Declaration::VariableDeclaration(declaration) => Some(&**declaration),
                _ => None,
            },
            _ => None,
        };

        // The compiler marks `await using` with its const flag as well.
        if let Some(declaration) = declaration.filter(|declaration| {
            matches!(
                declaration.kind,
                VariableDeclarationKind::Const | VariableDeclarationKind::AwaitUsing
            )
        }) {
            for declarator in &declaration.declarations {
                binding_names(&declarator.id, &mut names);
            }
        }
    }

    names
}

fn binding_names(pattern: &BindingPattern<'_>, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => names.push(identifier.name.to_string()),
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                binding_names(&property.value, names);
            }

            if let Some(rest) = &object.rest {
                binding_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                binding_names(element, names);
            }

            if let Some(rest) = &array.rest {
                binding_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => binding_names(&assignment.left, names),
    }
}

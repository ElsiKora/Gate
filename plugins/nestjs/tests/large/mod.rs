//! A synthetic service tree of any size in the conventional layout, for counting the work one call
//! does: `modules` modules, each with entities, DTOs, enums, interfaces, constants, controllers,
//! services, barrels and tests, plus the shared owners, migrations and scripts. The sources are
//! written for the count, not to pass or fail a rule.

#![allow(
    dead_code,
    reason = "each test crate uses its own subset of the helpers"
)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, RuleRun, TreeHost};

/// A tree that counts how often each path is read.
pub struct Counted {
    files: BTreeMap<String, Vec<u8>>,
    reads: RefCell<BTreeMap<String, usize>>,
}

impl Counted {
    #[must_use]
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            files,
            reads: RefCell::new(BTreeMap::new()),
        }
    }

    /// The number of tracked files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// The reads since the last [`Counted::take`], by path.
    pub fn take(&self) -> BTreeMap<String, usize> {
        std::mem::take(&mut *self.reads.borrow_mut())
    }
}

impl TreeHost for Counted {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        *self.reads.borrow_mut().entry(path.to_owned()).or_default() += 1;
        self.files.get(path).cloned()
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Counted {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// The rules of the plugin that run with their default options, by name.
#[must_use]
pub fn rules() -> Vec<String> {
    let tiny = Counted::new(tree(1));
    gate_sdk::describe(&nestjs::PLUGIN)
        .rules
        .into_iter()
        .map(|rule| rule.name)
        .filter(|name| check(&tiny, &[name.as_str()]).is_ok())
        .collect()
}

/// One call of the rules `names` with default options over `host`.
///
/// # Errors
/// A rule's error.
pub fn check(host: &Counted, names: &[&str]) -> gate_sdk::Result<Vec<Finding>> {
    let request = CheckRequest {
        project: String::new(),
        rules: names
            .iter()
            .map(|name| RuleRun {
                name: (*name).to_owned(),
                options: "{}".to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: "{}".to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
    };
    gate_sdk::check(&nestjs::PLUGIN, host, &request)
}

/// A service of `modules` modules, about 52 files each, plus about 50 shared files, an entity
/// that does not parse and one that is not UTF-8 text.
#[must_use]
pub fn tree(modules: usize) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut add = |path: String, text: String| {
        files.insert(path, text.into_bytes());
    };

    add(
        "package.json".to_owned(),
        r#"{ "name": "service", "dependencies": { "@nestjs/common": "11.0.0", "typeorm": "0.3.20" } }"#
            .to_owned(),
    );
    add(
        "tsconfig.json".to_owned(),
        r#"{ "compilerOptions": { "paths": { "@modules/*": ["./src/modules/*"], "@shared/*": ["./src/shared/*"], "@common/*": ["./src/common/*"] } } }"#
            .to_owned(),
    );
    add(
        "src/main.ts".to_owned(),
        "import { NestFactory } from '@nestjs/core';\nimport { AppModule } from './app.module';\n\
         async function bootstrap(): Promise<void> { const app = await NestFactory.create(AppModule); await app.listen(3000); }\n\
         void bootstrap();\n"
            .to_owned(),
    );
    let imports: Vec<String> = (0..modules)
        .map(|module| {
            format!(
                "import {{ {} }} from '@modules/{}';\n",
                class(module, "Module"),
                name(module)
            )
        })
        .collect();
    let list: Vec<String> = (0..modules).map(|module| class(module, "Module")).collect();
    add(
        "src/app.module.ts".to_owned(),
        format!(
            "import {{ Module }} from '@nestjs/common';\n{}@Module({{ imports: [{}] }})\nexport class AppModule {{}}\n",
            imports.concat(),
            list.join(", ")
        ),
    );
    for owner in ["common", "shared", "config", "runtime", "infrastructure"] {
        for index in 0..8 {
            add(
                format!("src/{owner}/utility/{owner}-helper-{index}.utility.ts"),
                format!(
                    "export function {owner}Helper{index}(value: string): string {{\n  return value.trim().toLowerCase();\n}}\n"
                ),
            );
        }
        add(
            format!("src/{owner}/index.ts"),
            (0..8).fold(String::new(), |mut text, index| {
                let _ = writeln!(
                    text,
                    "export * from './utility/{owner}-helper-{index}.utility';"
                );
                text
            }),
        );
        add(
            format!("src/{owner}/constant/{owner}.constant.ts"),
            format!(
                "export const {}_LIMIT: number = 10;\n",
                owner.to_uppercase()
            ),
        );
    }
    for index in 0..modules {
        add(
            format!(
                "src/migration/generated/17000000{index:05}-create-{}.migration.ts",
                name(index)
            ),
            format!(
                "import type {{ MigrationInterface, QueryRunner }} from 'typeorm';\n\
                 export class Create{}17000000{index:05} implements MigrationInterface {{\n\
                   public async up(queryRunner: QueryRunner): Promise<void> {{ await queryRunner.query('CREATE TABLE \"{}\" (\"id\" uuid)'); }}\n\
                   public async down(queryRunner: QueryRunner): Promise<void> {{ await queryRunner.query('DROP TABLE \"{}\"'); }}\n\
                 }}\n",
                pascal(index),
                name(index),
                name(index)
            ),
        );
    }
    for module in 0..modules {
        module_files(module, &mut add);
    }
    let entities = format!("src/modules/{}/domain/entity", name(0));
    files.insert(
        format!("{entities}/broken.entity.ts"),
        b"export class Broken {\n  @Column( name!: string;\n".to_vec(),
    );
    files.insert(
        format!("{entities}/latin.entity.ts"),
        b"export const label: string = '\xe9t\xe9';\n".to_vec(),
    );
    files
}

fn module_files(module: usize, add: &mut impl FnMut(String, String)) {
    let root = format!("src/modules/{}", name(module));
    let entities: Vec<String> = (0..4)
        .map(|entity| format!("{}-item{entity}", name(module)))
        .collect();

    for index in 0..entities.len() {
        entity_files(module, &root, &entities, index, add);
    }
    for kind in ["status", "kind"] {
        add(
            format!("{root}/domain/enum/{}-{kind}.enum.ts", name(module)),
            format!(
                "export enum E{}{} {{\n  ACTIVE = 'ACTIVE',\n  INACTIVE = 'INACTIVE',\n}}\n",
                pascal(module),
                capital(kind)
            ),
        );
    }
    add(
        format!("{root}/domain/constant/{}.constant.ts", name(module)),
        format!(
            "export const {}_PAGE_SIZE: number = 50;\n",
            name(module).to_uppercase().replace('-', "_")
        ),
    );
    for folder in [
        "domain/entity",
        "domain/dto",
        "public/controller",
        "public/service",
    ] {
        add(
            format!("{root}/{folder}/index.ts"),
            "export {};\n".to_owned(),
        );
    }
    let controllers: Vec<String> = (0..4)
        .map(|index| format!("{}Item{index}Controller", pascal(module)))
        .collect();
    let services: Vec<String> = (0..4)
        .map(|index| format!("{}Item{index}Service", pascal(module)))
        .collect();
    let imports: Vec<String> = entities
        .iter()
        .flat_map(|entity| {
            [
                format!("export * from './public/controller/{entity}.controller';\n"),
                format!("export * from './public/service/{entity}.service';\n"),
            ]
        })
        .collect();
    add(
        format!("{root}/{}.module.ts", name(module)),
        format!(
            "import {{ Module }} from '@nestjs/common';\nimport {{ {} }} from './index';\n\
             @Module({{ controllers: [{}], providers: [{}] }})\nexport class {} {{}}\n",
            controllers
                .iter()
                .chain(&services)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            controllers.join(", "),
            services.join(", "),
            class(module, "Module")
        ),
    );
    add(
        format!("{root}/index.ts"),
        format!(
            "{}export * from './{}.module';\n",
            imports.concat(),
            name(module)
        ),
    );
}

/// The files of one entity of a module: the entity, its DTOs, controller, services, interface,
/// adapter and specs.
fn entity_files(
    module: usize,
    root: &str,
    entities: &[String],
    index: usize,
    add: &mut impl FnMut(String, String),
) {
    let entity = &entities[index];
    let entity_class = format!("{}Item{index}Entity", pascal(module));
    let related = format!("{}Item{}Entity", pascal(module), (index + 1) % 4);
    add(
        format!("{root}/domain/entity/{entity}.entity.ts"),
        format!(
            "import {{ Column, CreateDateColumn, Entity, ManyToOne, PrimaryGeneratedColumn }} from 'typeorm';\n\
             import {{ {related} }} from './{}.entity';\n\
             import {{ E{}Status }} from '../enum/{}-status.enum';\n\
             @Entity('{}')\nexport class {entity_class} {{\n\
               @PrimaryGeneratedColumn('uuid', {{ comment: 'The identifier.' }}) id!: string;\n\
               @Column({{ type: 'varchar', length: 128, comment: 'The name.' }}) name!: string;\n\
               @Column({{ type: 'enum', enum: E{}Status, comment: 'The status.' }}) status!: E{}Status;\n\
               @CreateDateColumn({{ comment: 'When it was created.' }}) createdAt!: Date;\n\
               @ManyToOne(() => {related}, {{ nullable: true }}) parent?: {related};\n\
             }}\n",
            entities[(index + 1) % 4],
            pascal(module),
            name(module),
            entity.replace('-', "_"),
            pascal(module),
            pascal(module)
        ),
    );
    for action in ["create", "update", "response"] {
        add(
            format!("{root}/domain/dto/{entity}-{action}.dto.ts"),
            format!(
                "import {{ ApiProperty }} from '@nestjs/swagger';\nimport {{ IsString, MaxLength }} from 'class-validator';\n\
                 export class {}Item{index}{}Dto {{\n\
                   @ApiProperty({{ description: 'The name.' }}) @IsString() @MaxLength(128) name!: string;\n\
                 }}\n",
                pascal(module),
                capital(action)
            ),
        );
    }
    add(
        format!("{root}/public/controller/{entity}.controller.ts"),
        format!(
            "import {{ Controller, Get, Param }} from '@nestjs/common';\n\
             import {{ ApiController }} from '@elsikora/nestjs-crud-automator';\n\
             import {{ {entity_class} }} from '../../domain/entity/{entity}.entity';\n\
             import {{ {service} }} from '../service/{entity}.service';\n\
             @ApiController<{entity_class}>({{ entity: {entity_class}, name: '{}Item{index}' }})\n\
             @Controller('{entity}')\nexport class {}Item{index}Controller {{\n\
               constructor(public service: {service}) {{}}\n\
               @Get('summary/:id') public summary(@Param('id') id: string): Promise<string> {{ return this.service.summary(id); }}\n\
             }}\n",
            pascal(module),
            pascal(module),
            service = format!("{}Item{index}Service", pascal(module))
        ),
    );
    add(
        format!("{root}/public/service/{entity}.service.ts"),
        format!(
            "import {{ Injectable }} from '@nestjs/common';\nimport {{ InjectRepository }} from '@nestjs/typeorm';\n\
             import {{ Repository }} from 'typeorm';\nimport {{ commonHelper0 }} from '@common/utility/common-helper-0.utility';\n\
             import {{ {entity_class} }} from '../../domain/entity/{entity}.entity';\n\
             @Injectable()\nexport class {}Item{index}Service {{\n\
               constructor(@InjectRepository({entity_class}) private readonly repository: Repository<{entity_class}>) {{}}\n\
               public async summary(id: string): Promise<string> {{\n\
                 const found = await this.repository.findOne({{ where: {{ id }} }});\n\
                 return commonHelper0(found?.name ?? '');\n\
               }}\n\
             }}\n",
            pascal(module)
        ),
    );
    support_files(module, root, entity, &entity_class, index, add);
}

/// The files beside an entity's controller and service: a sync service, the interface, the
/// adapter and the specs.
fn support_files(
    module: usize,
    root: &str,
    entity: &str,
    entity_class: &str,
    index: usize,
    add: &mut impl FnMut(String, String),
) {
    add(
        format!("{root}/internal/service/{entity}-sync.service.ts"),
        format!(
            "import {{ Injectable, Logger }} from '@nestjs/common';\n\
             @Injectable()\nexport class {}Item{index}SyncService {{\n\
               private readonly logger: Logger = new Logger('{}Item{index}SyncService');\n\
               public run(values: Array<string>): number {{\n\
                 let total = 0;\n    for (const value of values) {{ if (value.length > 3) {{ total += value.length; }} else {{ this.logger.warn(value); }} }}\n\
                 return total;\n  }}\n\
             }}\n",
            pascal(module),
            pascal(module)
        ),
    );
    add(
        format!("{root}/domain/interface/{entity}.interface.ts"),
        format!(
            "export interface I{}Item{index} {{\n  id: string;\n  name: string;\n}}\n",
            pascal(module)
        ),
    );
    add(
        format!("{root}/integration/adapter/{entity}.adapter.ts"),
        format!(
            "import {{ Injectable }} from '@nestjs/common';\n@Injectable()\nexport class {}Item{index}Adapter {{\n\
               public send(payload: Record<string, unknown>): string {{ return JSON.stringify(payload); }}\n}}\n",
            pascal(module)
        ),
    );
    for kind in ["controller", "service", "e2e"] {
        add(
            format!("test/modules/{}/{entity}.{kind}.spec.ts", name(module)),
            format!(
                "import {{ {entity_class} }} from '../../../src/modules/{}/domain/entity/{entity}.entity';\n\
                 describe('{entity} {kind}', () => {{ it('builds', () => {{ expect(new {entity_class}()).toBeDefined(); }}); }});\n",
                name(module)
            ),
        );
    }
}

fn name(module: usize) -> String {
    format!("area{module:03}")
}

fn pascal(module: usize) -> String {
    format!("Area{module:03}")
}

fn class(module: usize, suffix: &str) -> String {
    format!("{}{suffix}", pascal(module))
}

fn capital(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

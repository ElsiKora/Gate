# The `layout` block

Every rule of the plugin reads one block: where the service keeps its sources, what its folders mean,
and which packages and decorators the rules recognise. A service that departs from the conventional
layout says so once, here.

```toml
[plugins.nestjs]
source = "…"
options = { layout = { source-root = "app", test-root = "spec" } }
```

Every key is optional. A key the block does not know, or a value of the wrong type, stops the run with
an error that names the key.

## Paths and names

Folders are relative to the project root and written in their normalized form: no `./`, `.` or `..`
segment, no doubled or trailing `/`; `""` is the project root. A name (the lists of folder names,
`domain-folder`) is one path segment, without `/`. A value in another form stops the run with an error
that shows the form to write.

## Keys that derive from `source-root`

Three keys follow `source-root` when they are not set. With `P` the source root followed by `/` (empty
when the source root is the project root):

| Key | Type | Derived default | Meaning |
| --- | --- | --- | --- |
| `modules-root` | folder | `P` + `modules` | The folder that holds one folder per module. |
| `infrastructure-folder` | folder | `P` + `infrastructure` | The infrastructure owner. |
| `path-aliases` | map of alias to folder | `"@<owner>"` to `P` + `<owner>`, for each of `source-owners` | Import aliases: an alias matches a specifier that equals it or starts with it and `/`, and the matched part is replaced by the folder. |

A key that is set keeps its value whatever `source-root` is. Rule options that name a path below these
folders derive from them the same way when they are not set; each rule's page says which.

## The other keys

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `source-root` | folder | `"src"` | The application source root. |
| `test-root` | folder | `"test"` | The test root. |
| `scripts-root` | folder | `"scripts"` | The folder of repository scripts. |
| `cdk-root` | folder | `"infrastructure/cdk"` | The infrastructure-as-code project. |
| `cdk-code-folders` | list of folders | `["bin", "lib", "scripts"]` | The folders of the infrastructure-as-code project that hold its code. |
| `source-owners` | list of names | `["common", "config", "infrastructure", "migration", "modules", "runtime", "shared"]` | The top-level owners below the source root. |
| `module-planes` | list of names | `["domain", "integration", "internal", "public"]` | The planes a module divides into. |
| `domain-folder` | name | `"domain"` | The plane that holds a module's domain. |
| `kind-folders` | list of names | `["constant", "dto", "function", "interface", "type", "utility"]` | Folders that group one kind of file. |
| `exclusive-kind-folders` | list of names | `["constant", "decorator", "dto", "enum", "function", "interface", "type", "utility"]` | Folders that hold only their own kind of file and no other kind folder. |
| `role-folders` | list of names | `["guard", "strategy", "subscriber"]` | Folders that hold one role of class. |
| `artifact-folders` | list of names | `["constant", "dto", "enum", "function", "guard", "interface", "middleware", "strategy", "subscriber", "type", "utility"]` | Folders that hold declarations rather than a module's own classes. |
| `test-support-folders` | list of names | `["enum", "fixture", "fixtures", "interface", "type"]` | Folders below the test root whose files are test support. |
| `test-support-kinds` | list of names | `["enum", "fixture", "interface", "type"]` | File kinds (`*.<kind>.ts`) below the test root that are test support. |
| `test-scenario-suffixes` | list of strings | `[".test.ts", ".case.ts"]` | File name endings of test scenarios. A scenario is never test support. |
| `automator-package` | string | `"@elsikora/nestjs-crud-automator"` | The CRUD automator package. |
| `nest-common-package` | string | `"@nestjs/common"` | The `NestJS` common package. |
| `nest-core-package` | string | `"@nestjs/core"` | The `NestJS` core package. |
| `swagger-package` | string | `"@nestjs/swagger"` | The `OpenAPI` package of `NestJS`. |
| `typeorm-package` | string | `"typeorm"` | The `TypeORM` package. |
| `validator-package` | string | `"class-validator"` | The validation package. |
| `generated-routes` | list of strings | `["CREATE", "DELETE", "GET", "GET_LIST", "PARTIAL_UPDATE", "UPDATE"]` | The route kinds the automator generates for a controller. |
| `typeorm-entity-decorators` | list of strings | `["Entity", "ChildEntity"]` | The decorators that declare an entity. |
| `typeorm-column-decorators` | list of strings | `["Column", "CreateDateColumn", "DeleteDateColumn", "ObjectIdColumn", "PrimaryColumn", "PrimaryGeneratedColumn", "TreeLevelColumn", "UpdateDateColumn", "VersionColumn", "ViewColumn"]` | The decorators that declare a column. |
| `typeorm-relation-decorators` | list of strings | `["ManyToMany", "ManyToOne", "OneToMany", "OneToOne"]` | The decorators that declare a relation. |
| `typeorm-other-persisted-decorators` | list of strings | `["TreeChildren", "TreeParent", "VirtualColumn"]` | The other decorators that declare a persisted property. |

## The skip list

The plugin has no skip list of its own. A walk that skips folders, the infrastructure-as-code
project's included, skips a file in a folder that `[languages.typescript] skip-folders` names; a
folder is listed only when it holds a file that is not skipped. Entries match the way `.gitignore`
does: a name without `/` at any depth, an entry with `/` from the project root. The language's default
is `.git`, `node_modules`, `/build`, `/coverage` and `/dist`; see `[languages]` in the configuration
guide. The walk of the infrastructure-as-code project below `cdk-root` skips the same list; its
build output (`cdk.out`) is skipped only when the list names it, as `infrastructure/cdk/cdk.out` or
`cdk.out`, so a project that tracks it puts it there. Four walks skip no folder, and their pages say
so: `asset-storage`, the `advisory-lock-manifest` markers, the files below `modules-root` that
`automator-metadata` reads for its inventories, and `error-logging`.

## Test files

A file below `test-root` is a test scenario when its name ends with one of `test-scenario-suffixes`.
It is test support when it is no scenario and no declaration file, and it either sits below one of
`test-support-folders` or is named `*.<kind>.ts` for one of `test-support-kinds`.

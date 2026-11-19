//! `nestjs/owner-boundaries` against synthetic source trees.

mod host;

use host::{Row, SKIP_DIST, check, error, paths_with, row, rows, run};

const RULE: &str = "owner-boundaries";
const OPTIONS: &str = r#"{
    "owner-roots": [
        "src/modules/domain",
        "src/modules/integration",
        "src/modules/internal",
        "src/modules/public",
        "src/infrastructure",
        "src/shared"
    ],
    "api-owners": ["src/modules/integration", "src/modules/internal", "src/modules/public"],
    "domain-owner": "src/modules/domain",
    "internal-owner": "src/modules/internal",
    "plane-routes": ["public", "internal", "integration"],
    "retired-routes": ["legacy", "back-office"],
    "internal-base": {
        "modules": ["@shared/constant/route.constant", "@shared/constant"],
        "export": ["ROUTE_CONSTANT", "INTERNAL"]
    },
    "flattened-resources": ["library/shelf-book", "author-key"],
    "flat-key-controller-suffix": "-key.controller.ts"
}"#;

const NEST: &str = "import { Controller } from \"@nestjs/common\";";
const CRUD: &str = "import { ApiController } from \"@elsikora/nestjs-crud-automator\";";
const BASE: &str = "import { ROUTE_CONSTANT } from \"@shared/constant/route.constant\";";

fn finding(aspect: &str, path: &str, message: &str) -> Row {
    row(aspect, path, None, &format!("{path} {message}"))
}

fn barrel(path: &str, target: &str) -> Row {
    finding(
        "owner-barrel",
        path,
        &format!("re-exports {target} source across its owner barrel"),
    )
}

fn forwarding(path: &str, name: &str) -> Row {
    finding(
        "domain-module-forwarding",
        path,
        &format!("forwards domain module {name} across an owner boundary"),
    )
}

fn controller_owner(path: &str) -> Row {
    finding(
        "controller-owner",
        path,
        "declares a controller outside an API owner boundary",
    )
}

fn plane(path: &str) -> Row {
    finding(
        "api-plane-route",
        path,
        "hardcodes an API plane route prefix instead of using plane constants",
    )
}

fn base(path: &str) -> Row {
    finding(
        "api-plane-route",
        path,
        "must compose its route from ROUTE_CONSTANT.INTERNAL",
    )
}

fn stale(path: &str) -> Row {
    finding(
        "api-plane-route",
        path,
        "contains a stale hardcoded API owner route",
    )
}

fn manager(path: &str, parameter: &str) -> Row {
    finding(
        "entity-manager-exposure",
        path,
        &format!("exposes native EntityManager in public method {parameter}"),
    )
}

fn flat_key(path: &str) -> Row {
    finding(
        "nested-resource",
        path,
        "uses a flat key controller filename; nest key controllers under an owning key folder",
    )
}

fn flat_path(path: &str) -> Row {
    finding(
        "nested-resource",
        path,
        "flattens a child resource; nest it under its owning resource",
    )
}

fn flat_route(path: &str) -> Row {
    finding(
        "nested-resource",
        path,
        "declares a flat child-resource route; nest it under its owning resource",
    )
}

/// A controller file: the imports, then the declarations and the decorated class.
fn controller(imports: &[&str], body: &str) -> String {
    format!("{} {body}\n", imports.join(" "))
}

#[test]
fn barrels_that_re_export_another_owner_are_reported() {
    let a = "src/modules/internal/a/index.ts";
    let b = "src/modules/internal/b/index.ts";
    let c = "src/modules/internal/c/index.ts";
    let d = "src/modules/internal/d/index.ts";
    let shared = "src/shared/index.ts";

    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (a, "export { BookService } from \"@modules/domain/book\";\n"),
                (b, "export { BookService } from \"../../domain/book\";\n"),
                (
                    c,
                    "export { BookService } from \"@modules/internal/../domain/book\";\n"
                ),
                (
                    d,
                    "export * from \"@modules/domain/a\"; export * as B from \"../../public/b.ts\"; \
                     export type { C } from \"@modules/integration/c\"; export * from \"@modules/domain/a\";\n",
                ),
                (shared, "export * from \"@infrastructure/storage\";\n"),
            ]
        ),
        [
            barrel(a, "src/modules/domain"),
            barrel(b, "src/modules/domain"),
            barrel(c, "src/modules/domain"),
            barrel(d, "src/modules/domain"),
            barrel(d, "src/modules/public"),
            barrel(d, "src/modules/integration"),
            barrel(shared, "src/infrastructure"),
        ]
    );
}

#[test]
fn barrels_of_their_own_owner_empty_re_exports_and_packages_pass() {
    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    "src/modules/internal/book/index.ts",
                    "export { Local } from \"./local\"; export {} from \"@modules/domain/empty\"; \
                     export * from \"./child\"; export { A } from \"@modules/internal/a\"; \
                     export * from \"typeorm\"; export * from \"/absolute/path\";\n",
                ),
                (
                    "src/modules/internal/book/barrel.ts",
                    "export * from \"@modules/domain/book\";\n",
                ),
                (
                    "src/runtime/index.ts",
                    "export * from \"@modules/domain/book\";\n"
                ),
            ]
        ),
        []
    );
}

#[test]
fn modules_that_export_domain_modules_from_other_owners_are_reported() {
    let infrastructure = "src/infrastructure/forward-c/module.ts";
    let internal = "src/modules/internal/forward-a/module.ts";
    let public = "src/modules/public/forward-b/module.ts";
    let optional = "src/modules/public/optional-c/module.ts";
    let forward = "src/modules/public/optional-d/module.ts";

    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    internal,
                    "import { Module } from \"@nestjs/common\"; import { BookModule } from \"@modules/domain/book\"; \
                     @Module({ imports: [BookModule], exports: [BookModule] }) class InternalBookModule {}\n",
                ),
                (
                    public,
                    "import { Module as NativeModule } from \"@nestjs/common\"; \
                     import { AuthorModule as Author } from \"@modules/domain/author/module\"; \
                     const additional = [Author]; const exported = [...additional]; \
                     const unrelated = { exports: [] }; const config = { exports: exported }; \
                     @NativeModule(config) class PublicAuthorModule {}\n",
                ),
                (
                    infrastructure,
                    "import * as nest from \"@nestjs/common\"; \
                     import * as domain from \"../../modules/domain/author/module\"; \
                     @nest.Module({ exports: [nest.forwardRef(() => domain.AuthorModule)] }) class Example {}\n",
                ),
                (
                    optional,
                    "import { Module } from \"@nestjs/common\"; import { LibraryModule } from \"@modules/domain/library\"; \
                     @(Module?.({ exports: [LibraryModule] })) class Example {}\n",
                ),
                (
                    forward,
                    "import * as n from \"@nestjs/common\"; import { LibraryModule } from \"@modules/domain/library\"; \
                     @n.Module({ exports: [n?.forwardRef(() => LibraryModule)] }) class Example {}\n",
                ),
            ]
        ),
        [
            forwarding(infrastructure, "AuthorModule"),
            forwarding(internal, "BookModule"),
            forwarding(public, "AuthorModule"),
            forwarding(optional, "LibraryModule"),
            forwarding(forward, "LibraryModule"),
        ]
    );
}

#[test]
fn domain_composition_imports_decoys_and_type_only_imports_pass() {
    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    "src/modules/internal/book/module.ts",
                    "import { Module } from \"@nestjs/common\"; import { BookModule } from \"@modules/domain/book\"; \
                     @Module({ imports: [BookModule] }) class InternalBookModule {}\n",
                ),
                (
                    "src/modules/domain/book/module.ts",
                    "import { Module } from \"@nestjs/common\"; import { AuthorModule } from \"@modules/domain/author\"; \
                     @Module({ exports: [AuthorModule] }) class BookModule {}\n",
                ),
                (
                    "src/modules/public/decoy/module.ts",
                    "import { Module } from \"@nestjs/common\"; import { AuthorModule } from \"@modules/domain/author/module\"; \
                     const decoy = { exports: [AuthorModule] }; // @Module({ exports: [AuthorModule] })\n\
                     function local(Module: Function) { @Module({ exports: [AuthorModule] }) class Local {} } \
                     @Module({ imports: [AuthorModule], exports: [\"AuthorModule\"] }) class Example {}\n",
                ),
                (
                    "src/modules/public/typed/module.ts",
                    "import { Module } from \"@nestjs/common\"; import type { AuthorModule } from \"@modules/domain/author/module\"; \
                     @Module({ exports: [AuthorModule as never] }) class Example {}\n",
                ),
                (
                    "src/modules/public/service/helper.ts",
                    "import { Module } from \"@nestjs/common\"; import { AuthorModule } from \"@modules/domain/author\"; \
                     @Module({ exports: [AuthorModule] }) class Example {}\n",
                ),
            ]
        ),
        []
    );
}

#[test]
fn controllers_outside_api_owners_are_reported() {
    let infrastructure = "src/infrastructure/provider/controller.ts";
    let runtime = "src/runtime/controller.ts";

    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    infrastructure,
                    &controller(
                        &[NEST],
                        "@Controller(\"callback\") class CallbackController {}"
                    ),
                ),
                (
                    runtime,
                    "import * as nest from \"@nestjs/common\"; @nest.Controller({ path: \"health\" }) class Health {}\n",
                ),
                (
                    "src/modules/integration/provider/controller.ts",
                    &controller(
                        &[NEST],
                        "@Controller(\"callback\") class CallbackController {}"
                    ),
                ),
            ]
        ),
        [controller_owner(infrastructure), controller_owner(runtime)]
    );
}

/// A public controller's path.
fn public(name: &str) -> String {
    format!("src/modules/public/{name}/controller.ts")
}

/// Controllers whose routes start with a plane or retired segment, one form each.
fn route_forms() -> Vec<(String, String)> {
    let p = public;
    vec![
        (
            p("a-optional"),
            "import * as n from \"@nestjs/common\"; @(n?.Controller(\"public/x\")) class X {}\n"
                .to_owned(),
        ),
        (
            p("b-optional"),
            controller(&[NEST], "@(Controller?.(\"legacy\")) class X {}"),
        ),
        (
            p("c-public"),
            controller(&[NEST], "@Controller(\"public/item\") class X {}"),
        ),
        (
            p("d-internal"),
            controller(&[NEST], "@Controller(\"internal\") class X {}"),
        ),
        (
            p("e-alias"),
            "import { Controller as Route } from \"@nestjs/common\"; @Route(\"internal/item\") class X {}\n"
                .to_owned(),
        ),
        (
            p("f-options"),
            "import { ApiController as Route } from \"@elsikora/nestjs-crud-automator\"; \
             const options = { path: \"internal/item\" }; @Route(options) class X {}\n"
                .to_owned(),
        ),
        (
            p("g-namespace"),
            "import * as crud from \"@elsikora/nestjs-crud-automator\"; const options = { path: \"internal/item\" }; \
             const alias = options; @crud.ApiController(alias) class X {}\n"
                .to_owned(),
        ),
        (
            p("h-concat"),
            controller(
                &[NEST],
                "declare const resource: string; const path = \"internal/\" + resource; @Controller(path) class X {}",
            ),
        ),
        (
            p("i-template"),
            controller(
                &[NEST],
                "declare const resource: string; @Controller(`internal/${resource}`) class X {}",
            ),
        ),
        (
            p("j-computed"),
            controller(
                &[CRUD],
                "const KEY = \"path\"; @ApiController({ [KEY]: \"public/health\" }) class X {}",
            ),
        ),
        (
            p("k-spread"),
            controller(
                &[CRUD],
                "const initial = { path: \"legacy/old\" }; const config = { ...initial, path: \"public/health\" }; \
                 @ApiController(config) class X {}",
            ),
        ),
        (
            p("l-array"),
            controller(&[NEST], "@Controller([\"public/a\", \"health\"]) class X {}"),
        ),
        (
            p("m-legacy"),
            controller(&[NEST], "@Controller(\"legacy/item\") class X {}"),
        ),
        (
            p("n-office"),
            controller(&[NEST], "@Controller(\"back-office\") class X {}"),
        ),
        (
            p("o-template"),
            controller(
                &[NEST, BASE],
                "@Controller(`legacy/${ROUTE_CONSTANT.INTERNAL}`) class X {}",
            ),
        ),
        (
            "src/modules/integration/plane/controller.ts".to_owned(),
            controller(&[NEST], "@Controller(\"integration/item\") class X {}"),
        ),
    ]
}

#[test]
fn literal_plane_and_retired_route_prefixes_are_reported_through_every_form() {
    let p = public;
    let files = route_forms();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();

    assert_eq!(
        check(RULE, OPTIONS, &files),
        [
            plane("src/modules/integration/plane/controller.ts"),
            plane(&p("a-optional")),
            stale(&p("b-optional")),
            plane(&p("c-public")),
            plane(&p("d-internal")),
            plane(&p("e-alias")),
            plane(&p("f-options")),
            plane(&p("g-namespace")),
            plane(&p("h-concat")),
            plane(&p("i-template")),
            plane(&p("j-computed")),
            plane(&p("k-spread")),
            plane(&p("l-array")),
            stale(&p("m-legacy")),
            stale(&p("n-office")),
            stale(&p("o-template")),
        ]
    );
}

#[test]
fn internal_routes_that_do_not_all_start_with_the_base_are_reported() {
    let i = |name: &str| format!("src/modules/internal/{name}/controller.ts");
    let files = [
        (
            i("array"),
            controller(
                &[NEST, BASE],
                "@Controller([`${ROUTE_CONSTANT.INTERNAL}/good`, \"internal/raw\"]) class X {}",
            ),
        ),
        (
            i("later"),
            controller(
                &[NEST, BASE],
                "const unused = ROUTE_CONSTANT.INTERNAL; @Controller(`item/${ROUTE_CONSTANT.INTERNAL}`) class X {}",
            ),
        ),
        (
            i("missing"),
            controller(&[NEST], "@Controller(\"item\") class X {}"),
        ),
        (i("none"), controller(&[NEST], "@Controller() class X {}")),
        (
            i("spread"),
            controller(
                &[CRUD, BASE, "import { runtime } from \"./external\";"],
                "const nested = { ...runtime }; const config = { path: ROUTE_CONSTANT.INTERNAL, ...nested }; \
                 @ApiController(config) class X {}",
            ),
        ),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();

    assert_eq!(
        check(RULE, OPTIONS, &files),
        [
            plane(&i("array")),
            base(&i("array")),
            base(&i("later")),
            base(&i("missing")),
            base(&i("none")),
            base(&i("spread")),
        ]
    );
}

#[test]
fn routes_built_from_the_base_and_unknown_or_shadowed_routes_pass() {
    let files = [
        (
            "src/modules/internal/item/controller.ts",
            "import { Controller } from \"@nestjs/common\"; import { ROUTE_CONSTANT as R } from \"@shared/constant\"; \
             @Controller(`${R.INTERNAL}/item`) class X {}\n"
                .to_owned(),
        ),
        (
            "src/modules/internal/concat/controller.ts",
            "import { ApiController } from \"@elsikora/nestjs-crud-automator\"; \
             import { ROUTE_CONSTANT as R } from \"@shared/constant/route.constant\"; \
             const prefix = R.INTERNAL; const path = prefix + \"/item\"; const config = { path }; \
             @ApiController(config) class X {}\n"
                .to_owned(),
        ),
        (
            "src/modules/internal/element/controller.ts",
            "import { ApiController } from \"@elsikora/nestjs-crud-automator\"; \
             import { ROUTE_CONSTANT as R } from \"@shared/constant/route.constant\"; \
             const path = R[\"INTERNAL\"] + \"/item\"; const config = { path }; @ApiController(config) class X {}\n"
                .to_owned(),
        ),
        (
            "src/modules/public/health/controller.ts",
            "import { Controller } from \"@nestjs/common\"; // @Controller(\"legacy/comment\")\n\
             const help = '@Controller(\"legacy/string\")'; const unrelated = { path: \"legacy/unrelated\", exports: [] }; \
             function local(Controller: Function) { @Controller(\"legacy/local\") class Local {} } \
             @Controller(\"health\") class Health {}\n"
                .to_owned(),
        ),
        (
            "src/modules/public/unknown/controller.ts",
            controller(
                &[CRUD, "import { path } from \"./external\";"],
                "@ApiController({ [path]: \"legacy/unknown\" }) class X {}",
            ),
        ),
        (
            "src/modules/public/shadowed/controller.ts",
            "function Controller(path: string) { return (_: unknown) => undefined; } @Controller(\"legacy/item\") class X {}\n"
                .to_owned(),
        ),
        (
            "src/modules/public/string/controller.ts",
            controller(&[CRUD], "@ApiController(\"public/item\") class X {}"),
        ),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();

    assert_eq!(check(RULE, OPTIONS, &files), []);
}

#[test]
fn flat_key_controllers_flattened_paths_and_flattened_internal_routes_are_reported() {
    let key = "src/modules/internal/author/reader-key.controller.ts";
    let entity = "src/modules/domain/library/shelf-book/entity.ts";
    let nested = "src/modules/internal/library/shelf-book/controller.ts";
    let service = "src/modules/internal/author-key/service.ts";
    let route_a = "src/modules/internal/flat-a/controller.ts";
    let route_b = "src/modules/internal/flat-b/controller.ts";

    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (key, "export {};\n"),
                (entity, "export {};\n"),
                (nested, "export {};\n"),
                (service, "export {};\n"),
                (
                    route_a,
                    &controller(
                        &[CRUD, BASE],
                        "const path = `${ROUTE_CONSTANT.INTERNAL}/author-key`; const options = { path }; \
                         @ApiController(options) class X {}",
                    ),
                ),
                (
                    route_b,
                    &controller(
                        &[CRUD, BASE],
                        "const path = ROUTE_CONSTANT.INTERNAL + \"/library/shelf-\" + \"book\"; \
                         const options = { path }; @ApiController(options) class X {}",
                    ),
                ),
            ]
        ),
        [
            flat_path(entity),
            flat_path(service),
            flat_key(key),
            flat_route(route_a),
            flat_route(route_b),
            flat_path(nested),
        ]
    );
}

#[test]
fn nested_resources_unknown_route_parts_and_decoys_pass() {
    let files = [
        (
            "src/modules/internal/author/key/controller.ts",
            controller(
                &[CRUD, BASE],
                "// historical path: `/library/shelf-book`\nconst help = \"author-key\"; \
                 const unused = { path: \"/library/shelf-book\" }; \
                 @ApiController({ path: `${ROUTE_CONSTANT.INTERNAL}/author/key` }) class X {}",
            ),
        ),
        (
            "src/modules/internal/library/controller.ts",
            controller(
                &[NEST, BASE],
                "declare const unknown: string; \
                 @Controller(ROUTE_CONSTANT.INTERNAL + \"/library/shelf-\" + unknown + \"book\") class X {}",
            ),
        ),
        (
            "src/modules/internal/key/controller.ts",
            "export {};\n".to_owned(),
        ),
        (
            "src/modules/public/key.controller.ts",
            "export {};\n".to_owned(),
        ),
        (
            "src/modules/public/Reader-key.controller.ts",
            "export {};\n".to_owned(),
        ),
        (
            "src/modules/public/library/shelf-books/controller.ts",
            "export {};\n".to_owned(),
        ),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();

    assert_eq!(check(RULE, OPTIONS, &files), []);
}

#[test]
fn public_domain_service_methods_holding_the_entity_manager_are_reported() {
    let d = |name: &str| format!("src/modules/domain/{name}/service.ts");
    let files = [
        (
            d("a"),
            "import type { EntityManager as Manager } from \"typeorm\"; export class X { mutate(manager: Manager) {} }\n",
        ),
        (
            d("b"),
            "import type * as orm from \"typeorm\"; export class X { mutate(manager: orm.EntityManager) {} }\n",
        ),
        (
            d("c"),
            "import type { EntityManager } from \"typeorm\"; type Options = { nested: { manager: EntityManager } }; \
             export class X { mutate(manager: Options) {} }\n",
        ),
        (
            d("d"),
            "import type { EntityManager } from \"typeorm\"; interface Options { next?: Options; manager: EntityManager }; \
             export class X { mutate(manager: Options) {} }\n",
        ),
        (
            d("e"),
            "import type { EntityManager } from \"typeorm\"; \
             export class X { mutate(manager: EntityManager | undefined) {} static other(...managers: EntityManager[]) {} }\n",
        ),
        (
            d("f"),
            "import type { EntityManager } from \"typeorm\"; interface Base<T> { value: T } \
             interface Options extends Base<EntityManager> {} export const helpers = { run(manager: Options) {} };\n",
        ),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), *text))
        .collect();

    assert_eq!(
        check(RULE, OPTIONS, &files),
        [
            manager(&d("a"), "mutate(manager)"),
            manager(&d("b"), "mutate(manager)"),
            manager(&d("c"), "mutate(manager)"),
            manager(&d("d"), "mutate(manager)"),
            manager(&d("e"), "mutate(manager)"),
            manager(&d("e"), "other(managers)"),
            manager(&d("f"), "run(manager)"),
        ]
    );
}

#[test]
fn hidden_local_foreign_and_non_service_entity_managers_pass() {
    let manager = "import type { EntityManager } from \"typeorm\"; export class X { mutate(manager: EntityManager) {} }\n";

    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    "src/modules/domain/local/service.ts",
                    "interface EntityManager { id: string }; export class X { mutate(manager: EntityManager) {} }\n",
                ),
                (
                    "src/modules/domain/foreign/service.ts",
                    "import type { EntityManager } from \"another-library\"; export class X { mutate(manager: EntityManager) {} }\n",
                ),
                (
                    "src/modules/domain/generic/service.ts",
                    "import type { EntityManager } from \"typeorm\"; export class X<EntityManager> { mutate(manager: EntityManager) {} }\n",
                ),
                (
                    "src/modules/domain/hidden/service.ts",
                    "import type { EntityManager } from \"typeorm\"; export class X { constructor(manager: EntityManager) {} \
                     private mutate(manager: EntityManager) {} protected other(manager: EntityManager) {} \
                     #internal(manager: EntityManager) {} get value(): EntityManager | undefined { return undefined; } }\n",
                ),
                (
                    "src/modules/domain/cycle/service.ts",
                    "interface Options { next?: Options }; export class X { mutate(manager: Options) {} }\n",
                ),
                (
                    "src/modules/domain/heritage/service.ts",
                    "import type { EntityManager } from \"typeorm\"; interface Options extends EntityManager {} \
                     export class X { mutate(manager: typeof import(\"typeorm\").EntityManager, other: Options) {} }\n",
                ),
                ("src/infrastructure/session/service.ts", manager),
                ("src/modules/domain/book/repository.ts", manager),
            ]
        ),
        []
    );
}

#[test]
fn the_import_options_and_the_service_file_name_change_what_is_read() {
    let options = r#"{
        "owner-roots": ["src/api", "src/core"],
        "api-owners": ["src/api"],
        "domain-owner": "src/core",
        "internal-owner": "src/api/internal",
        "plane-routes": ["public"],
        "internal-base": { "modules": [], "export": [] },
        "controller-decorators": [{ "package": "@scope/web", "name": "Route" }],
        "module-decorator": { "package": "@scope/web", "name": "Unit" },
        "entity-manager": { "package": "@scope/orm", "name": "Session" },
        "domain-service-file": "book.service.ts"
    }"#;
    let route = "src/core/route.ts";
    let module = "src/api/module.ts";
    let service = "src/core/book/book.service.ts";

    assert_eq!(
        check(
            RULE,
            options,
            &[
                (
                    "src/core/other.ts",
                    "import { Controller } from \"@nestjs/common\"; @Controller(\"public/x\") class X {}\n",
                ),
                (
                    module,
                    "import { Module } from \"@nestjs/common\"; import { BookModule } from \"src/core/book\"; \
                     @Module({ exports: [BookModule] }) class X {}\n",
                ),
                (
                    "src/core/book/service.ts",
                    "import type { EntityManager } from \"typeorm\"; export class X { run(manager: EntityManager) {} }\n",
                ),
                ("src/api/reader-key.controller.ts", "export {};\n"),
            ]
        ),
        []
    );

    let layout = r#"{ "layout": { "path-aliases": { "@core": "src/core" } } }"#;
    let findings = run(
        RULE,
        options,
        layout,
        &[
            (
                route,
                "import { Route } from \"@scope/web\"; @Route(\"public/book\") class X {}\n",
            ),
            (
                module,
                "import { Unit, forwardRef } from \"@scope/web\"; import { BookModule } from \"@core/book\"; \
                 @Unit({ exports: [forwardRef(() => BookModule)] }) class X {}\n",
            ),
            (
                service,
                "import type { Session } from \"@scope/orm\"; export class X { run(session: Session) {} }\n",
            ),
        ],
    )
    .expect("the rule runs");

    assert_eq!(
        rows(&findings),
        [
            forwarding(module, "BookModule"),
            row(
                "entity-manager-exposure",
                service,
                None,
                &format!("{service} exposes native Session in public method run(session)"),
            ),
            controller_owner(route),
            plane(route),
        ]
    );
}

#[test]
fn the_boundaries_are_required() {
    assert!(error(RULE, "{}", &[]).contains("owner-roots"));
}

#[test]
fn an_unknown_option_is_an_error() {
    let options = OPTIONS.replacen('{', "{ \"owners\": [],", 1);

    assert!(error(RULE, &options, &[]).contains("owners"));
}

#[test]
fn an_import_option_with_an_empty_package_is_an_error() {
    let options = OPTIONS.replacen(
        '{',
        "{ \"entity-manager\": { \"package\": \"\", \"name\": \"Session\" },",
        1,
    );

    assert!(error(RULE, &options, &[]).contains("`entity-manager` has an empty package"));
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let text = controller(
        &[NEST],
        "@Controller(\"callback\") class CallbackController {}",
    );
    let files = [
        ("src/infrastructure/dist/controller.ts", text.as_str()),
        ("src/shared/generated/controller.ts", text.as_str()),
    ];

    assert_eq!(
        paths_with(RULE, OPTIONS, SKIP_DIST, &files),
        [
            "src/infrastructure/dist/controller.ts",
            "src/shared/generated/controller.ts"
        ]
    );
}

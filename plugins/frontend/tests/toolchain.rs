//! The rules that read the toolchain files, run natively through the SDK against synthetic trees:
//! import-alias, route-config and packages.

mod support;

use support::{check, check_with, row, without};

const NEXT: &str = "nextjs";
const VITE: &str = "react-vite";
const LIBRARY: &str = "react-library";

/// A tsconfig with comments and a trailing comma, as TypeScript reads it.
const TSCONFIG: (&str, &str) = (
    "tsconfig.json",
    "{\n  // the one alias\n  \"compilerOptions\": {\n    /* src */ \"paths\": { \"@/*\": [\"./src/*\"], },\n  },\n}\n",
);

/// A Vite configuration with the `@` alias and `TanStack` Router's plugin, as the standard wants.
const VITE_CONFIG: (&str, &str) = (
    "vite.config.ts",
    "import path from \"node:path\";\nimport { defineConfig } from \"vite\";\nimport { tanstackRouter } from \"@tanstack/router-plugin/vite\";\nexport default defineConfig({\n  plugins: [tanstackRouter({ target: \"react\", routesDirectory: \"./src/app/routes\", generatedRouteTree: \"./src/app/route-tree.gen.ts\" })],\n  resolve: { alias: { \"@\": path.resolve(__dirname, \"./src\") } },\n});\n",
);

#[test]
fn import_alias_passes_the_one_alias_in_each_form() {
    let vite = [TSCONFIG, VITE_CONFIG];
    assert_eq!(check("import-alias", VITE, &vite), vec![]);

    let array = [
        TSCONFIG,
        (
            "vite.config.mts",
            "import { fileURLToPath, URL } from \"node:url\";\nexport default { resolve: { alias: [{ find: \"@\", replacement: fileURLToPath(new URL(\"./src\", import.meta.url)) }] } };\n",
        ),
    ];
    assert_eq!(check("import-alias", VITE, &array), vec![]);

    let plugin = [
        TSCONFIG,
        (
            "vite.config.ts",
            "import tsconfigPaths from \"vite-tsconfig-paths\";\nexport default { plugins: [tsconfigPaths()] };\n",
        ),
    ];
    assert_eq!(check("import-alias", VITE, &plugin), vec![]);

    let extended = [
        (
            "tsconfig.json",
            "{ \"extends\": \"./tsconfig.base.json\", \"include\": [\"src\"] }",
        ),
        (
            "tsconfig.base.json",
            "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"src/*\"] } } }",
        ),
    ];
    assert_eq!(check("import-alias", LIBRARY, &extended), vec![]);
    assert_eq!(check("import-alias", NEXT, &[TSCONFIG]), vec![]);
}

#[test]
fn import_alias_reports_a_missing_wrong_or_second_alias() {
    let no_paths = [("tsconfig.json", "{ \"compilerOptions\": {} }")];
    assert_eq!(
        check("import-alias", NEXT, &no_paths),
        vec![row("tsconfig", "tsconfig.json", None)]
    );

    let wrong = [(
        "tsconfig.json",
        "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./app/*\"], \"~/*\": [\"./src/*\"] } } }",
    )];
    let messages: Vec<String> =
        support::run_raw("import-alias", "{}", r#"{ "pack": "nextjs" }"#, &wrong)
            .expect("the rule runs")
            .into_iter()
            .map(|finding| finding.message)
            .collect();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(messages.iter().any(|message| message.contains("./app/*")));
    assert!(messages.iter().any(|message| message.contains("~/*")));

    // The Vite configuration without its alias, and with an alias to another folder.
    let no_alias = [
        TSCONFIG,
        (
            "vite.config.ts",
            "export default { resolve: { alias: { \"@\": \"/lib\" } } };\n",
        ),
    ];
    assert_eq!(
        check("import-alias", VITE, &no_alias),
        vec![row("vite", "vite.config.ts", None)]
    );
}

#[test]
fn route_config_holds_a_vite_app_to_the_route_folders() {
    let files = [TSCONFIG, VITE_CONFIG];
    assert_eq!(check("route-config", VITE, &files), vec![]);
    let legacy_name = [
        TSCONFIG,
        (
            "vite.config.ts",
            "import { TanStackRouterVite } from \"@tanstack/router-plugin/vite\";\nexport default { plugins: [TanStackRouterVite({ routesDirectory: \"src/app/routes\", generatedRouteTree: \"src/app/route-tree.gen.ts\" })] };\n",
        ),
    ];
    assert_eq!(check("route-config", VITE, &legacy_name), vec![]);

    let config = |options: &str| {
        format!(
            "import {{ tanstackRouter }} from \"@tanstack/router-plugin/vite\";\nexport default {{ plugins: [tanstackRouter({options})] }};\n"
        )
    };
    let elsewhere = config(
        r#"{ routesDirectory: "./src/routes", generatedRouteTree: "./src/app/route-tree.gen.ts" }"#,
    );
    assert_eq!(
        check(
            "route-config",
            VITE,
            &[TSCONFIG, ("vite.config.ts", elsewhere.as_str())]
        ),
        vec![row("routes-directory", "vite.config.ts", None)]
    );
    let defaults = config(r#"{ routesDirectory: "./src/app/routes" }"#);
    assert_eq!(
        check(
            "route-config",
            VITE,
            &[TSCONFIG, ("vite.config.ts", defaults.as_str())]
        ),
        vec![row("route-tree", "vite.config.ts", None)]
    );
    assert_eq!(
        check(
            "route-config",
            VITE,
            &[
                TSCONFIG,
                ("vite.config.ts", "export default { plugins: [] };\n")
            ]
        ),
        vec![row("plugin", "vite.config.ts", None)]
    );
    assert_eq!(
        check("route-config", VITE, &without(&files, "vite.config.ts")),
        vec![row("plugin", "vite.config.ts", None)]
    );

    let no_plugin = [TSCONFIG];
    assert_eq!(check("route-config", NEXT, &no_plugin), vec![]);
    assert_eq!(check("route-config", LIBRARY, &no_plugin), vec![]);
}

/// A `package.json` with `dependencies` and `devDependencies`.
fn manifest(dependencies: &[&str], development: &[&str]) -> String {
    let list = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("\"{name}\": \"1.0.0\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{{ \"name\": \"app\", \"dependencies\": {{ {} }}, \"devDependencies\": {{ {} }} }}",
        list(dependencies),
        list(development)
    )
}

#[test]
fn packages_allows_one_tool_per_job_and_the_pack_s_choices() {
    let vite = manifest(
        &[
            "react",
            "axios",
            "zustand",
            "@tanstack/react-router",
            "react-i18next",
            "i18next",
            "@emotion/react",
            "@emotion/styled",
        ],
        &["vite", "typescript"],
    );
    assert_eq!(
        check("packages", VITE, &[("package.json", vite.as_str())]),
        vec![]
    );
    let next = manifest(
        &["next", "next-intl", "ky"],
        &["tailwindcss", "@tailwindcss/postcss"],
    );
    assert_eq!(
        check("packages", NEXT, &[("package.json", next.as_str())]),
        vec![]
    );
    let library = manifest(&["react-i18next"], &["sass"]);
    assert_eq!(
        check("packages", LIBRARY, &[("package.json", library.as_str())]),
        vec![]
    );
}

#[test]
fn packages_reports_two_tools_of_a_job_and_a_tool_the_pack_does_not_use() {
    let two = manifest(
        &["axios", "zustand", "@tanstack/react-router"],
        &["ky", "@reduxjs/toolkit"],
    );
    let messages = support::messages("packages", VITE, &[("package.json", two.as_str())]);
    assert_eq!(
        check("packages", VITE, &[("package.json", two.as_str())]),
        vec![
            row("job", "package.json", None),
            row("job", "package.json", None),
        ]
    );
    assert!(
        messages.iter().any(|message| message.contains("axios, ky"))
            && messages
                .iter()
                .any(|message| message.contains("redux, zustand")),
        "{messages:?}"
    );

    let vite = manifest(&["react-router-dom"], &[]);
    assert_eq!(
        check("packages", VITE, &[("package.json", vite.as_str())]),
        vec![row("pack", "package.json", None)]
    );
    let next = manifest(&["next", "react-i18next"], &[]);
    assert_eq!(
        check("packages", NEXT, &[("package.json", next.as_str())]),
        vec![row("pack", "package.json", None)]
    );
    let library = manifest(&["wouter"], &[]);
    assert_eq!(
        check("packages", LIBRARY, &[("package.json", library.as_str())]),
        vec![row("pack", "package.json", None)]
    );

    let custom = r#"{ "jobs": { "charts": { "recharts": ["recharts"], "chartjs": ["chart.js", "react-chartjs-2"] } } }"#;
    let charts = manifest(&["recharts", "react-chartjs-2", "axios", "ky"], &[]);
    assert_eq!(
        check_with(
            "packages",
            VITE,
            custom,
            &[("package.json", charts.as_str())]
        ),
        vec![row("job", "package.json", None)]
    );
}

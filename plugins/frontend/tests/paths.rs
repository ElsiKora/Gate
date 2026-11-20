//! The rules that read paths only, run natively through the SDK against synthetic trees: layout,
//! file-names, typescript-sources, asset-placement and colocated-files, and the options errors of
//! the plugin.

mod support;

use support::{check, check_with, row, run_raw};

const NEXT: &str = "nextjs";
const VITE: &str = "react-vite";
const LIBRARY: &str = "react-library";

/// A Next.js app laid out as the standard says.
const NEXT_APP: &[(&str, &str)] = &[
    ("next.config.ts", "export default {};\n"),
    ("public/favicon.ico", ""),
    ("src/middleware.ts", "export function middleware() {}\n"),
    (
        "src/app/layout.tsx",
        "export default function Layout() { return null; }\n",
    ),
    (
        "src/app/providers.tsx",
        "export function Providers() { return null; }\n",
    ),
    ("src/app/globals.css", ""),
    (
        "src/app/(shop)/invoices/[invoiceId]/page.tsx",
        "export default function Page() { return null; }\n",
    ),
    (
        "src/app/@modal/(.)photo/page.tsx",
        "export default function Page() { return null; }\n",
    ),
    ("src/app/icon.png", ""),
    ("src/widgets/header/index.ts", "export {};\n"),
    (
        "src/widgets/header/ui/header.tsx",
        "export function Header() { return null; }\n",
    ),
    ("src/features/sign-in/index.ts", "export {};\n"),
    (
        "src/features/sign-in/model/use-sign-in.ts",
        "export function useSignIn() {}\n",
    ),
    ("src/features/sign-in/model/use-sign-in.test.ts", ""),
    ("src/entities/user/index.ts", "export {};\n"),
    ("src/entities/user/api/user-api.ts", "export {};\n"),
    ("src/entities/user/config/limits.ts", "export {};\n"),
    ("src/entities/user/lib/format.ts", "export {};\n"),
    ("src/shared/api/client.ts", "export {};\n"),
    ("src/shared/config/env.ts", "export {};\n"),
    ("src/shared/i18n/messages/en.json", "{}"),
    ("src/shared/assets/logo-dark.svg", ""),
    (
        "src/shared/ui/button/button.tsx",
        "export function Button() { return null; }\n",
    ),
    ("src/shared/ui/button/button.module.css", ""),
    ("src/shared/lib/types.d.ts", "export {};\n"),
];

#[test]
fn the_standard_tree_of_each_pack_passes_the_path_rules() {
    for rule in [
        "layout",
        "file-names",
        "typescript-sources",
        "asset-placement",
        "colocated-files",
    ] {
        assert_eq!(check(rule, NEXT, NEXT_APP), vec![], "{rule}, nextjs");
    }

    let vite = [
        ("index.html", ""),
        ("vite.config.ts", "export default {};\n"),
        ("src/main.tsx", "export {};\n"),
        ("src/vite-env.d.ts", "export {};\n"),
        ("src/app/route-tree.gen.ts", "export {};\n"),
        ("src/app/routes/__root.tsx", "export const Route = 1;\n"),
        ("src/app/routes/index.tsx", "export const Route = 1;\n"),
        (
            "src/app/routes/invoices/$invoiceId.tsx",
            "export const Route = 1;\n",
        ),
        (
            "src/app/routes/_auth/(admin)/settings.lazy.tsx",
            "export const Route = 1;\n",
        ),
        ("src/entities/invoice/index.ts", "export {};\n"),
    ];
    for rule in ["layout", "file-names", "typescript-sources"] {
        assert_eq!(check(rule, VITE, &vite), vec![], "{rule}, react-vite");
    }

    let library = [
        ("src/index.ts", "export {};\n"),
        (
            "src/ui/phone-input/phone-input.tsx",
            "export function PhoneInput() { return null; }\n",
        ),
        ("src/model/use-phone.ts", "export function usePhone() {}\n"),
        ("src/assets/flags.svg", ""),
    ];
    for rule in ["layout", "file-names", "asset-placement"] {
        assert_eq!(
            check(rule, LIBRARY, &library),
            vec![],
            "{rule}, react-library"
        );
    }
}

#[test]
fn layout_reports_every_departure_once_at_its_folder() {
    let files = [
        ("components/card.tsx", "export {};\n"),
        ("e2e/.keep", ""),
        ("src/modules/user/user.ts", "export {};\n"),
        ("src/modules/user/user-api.ts", "export {};\n"),
        ("src/features/UserCard/index.ts", "export {};\n"),
        ("src/features/no-index/ui/form.tsx", "export {};\n"),
        ("src/entities/user/index.ts", "export {};\n"),
        ("src/entities/user/user.ts", "export {};\n"),
        ("src/entities/user/helpers/format.ts", "export {};\n"),
        ("src/entities/loose.ts", "export {};\n"),
        ("src/shared/user/index.ts", "export {};\n"),
        ("src/shared/index.ts", "export {};\n"),
        ("src/router.tsx", "export {};\n"),
    ];
    assert_eq!(
        check("layout", NEXT, &files),
        vec![
            row("layer", "src/modules", None),
            row("root-file", "src/router.tsx", None),
            row("segment", "src/entities/user/helpers", None),
            row("segment", "src/entities/user/user.ts", None),
            row("shared", "src/shared/index.ts", None),
            row("shared", "src/shared/user", None),
            row("slice", "src/entities/loose.ts", None),
            row("slice", "src/features/UserCard", None),
            row("slice", "src/features/no-index", None),
            row("source-root", "components", None),
        ]
    );
}

#[test]
fn layout_holds_a_library_to_its_segments() {
    let files = [
        ("src/index.ts", "export {};\n"),
        ("src/components/card.tsx", "export {};\n"),
        ("src/main.tsx", "export {};\n"),
    ];
    assert_eq!(
        check("layout", LIBRARY, &files),
        vec![
            row("root-file", "src/main.tsx", None),
            row("shared", "src/components", None),
        ]
    );
}

#[test]
fn file_names_reads_each_router_syntax_only_in_its_route_folder() {
    let files = [
        ("src/shared/ui/UserCard.tsx", "export {};\n"),
        ("src/shared/lib/userCard.ts", "export {};\n"),
        ("src/shared/lib/user.helper.ts", "export {};\n"),
        ("src/shared/ui/Button/button.tsx", "export {};\n"),
        ("src/app/routes/[id].tsx", "export const Route = 1;\n"),
        ("src/app/routes/$id/index.tsx", "export const Route = 1;\n"),
    ];
    assert_eq!(
        check("file-names", VITE, &files),
        vec![
            row("file", "src/app/routes/[id].tsx", None),
            row("file", "src/shared/lib/userCard.ts", None),
            row("file", "src/shared/ui/UserCard.tsx", None),
            row("folder", "src/shared/ui/Button", None),
            row("suffix", "src/shared/lib/user.helper.ts", None),
        ]
    );

    // `[id]` is Next.js syntax; `$id` is not.
    let next = [
        ("src/app/[id]/page.tsx", "export default 1;\n"),
        ("src/app/$id/page.tsx", "export default 1;\n"),
        ("src/widgets/[id]/index.ts", "export {};\n"),
    ];
    assert_eq!(
        check("file-names", NEXT, &next),
        vec![
            row("folder", "src/app/$id", None),
            row("folder", "src/widgets/[id]", None),
        ]
    );
}

#[test]
fn typescript_sources_reports_javascript_under_src_only() {
    let files = [
        ("eslint.config.mjs", "export default [];\n"),
        ("src/shared/lib/format.js", "export {};\n"),
        ("src/shared/lib/types.d.ts", "export {};\n"),
    ];
    assert_eq!(
        check("typescript-sources", NEXT, &files),
        vec![row("", "src/shared/lib/format.js", None)]
    );
}

#[test]
fn asset_placement_allows_the_assets_segment_and_next_metadata_images() {
    let files = [
        ("public/robots.txt", ""),
        ("src/shared/assets/logo.svg", ""),
        ("src/app/icon.png", ""),
        ("src/app/blog/opengraph-image.jpg", ""),
        ("src/app/blog/favicon.ico", ""),
        ("src/features/auth/ui/logo.PNG", ""),
        ("src/assets/hero.jpg", ""),
    ];
    assert_eq!(
        check("asset-placement", NEXT, &files),
        vec![
            row("", "src/app/blog/favicon.ico", None),
            row("", "src/assets/hero.jpg", None),
            row("", "src/features/auth/ui/logo.PNG", None),
        ]
    );
    assert_eq!(
        check_with(
            "asset-placement",
            NEXT,
            r#"{ "extensions": ["svg"] }"#,
            &[("src/features/auth/ui/logo.png", "")]
        ),
        vec![]
    );
}

#[test]
fn colocated_files_reports_a_test_or_story_without_its_subject() {
    let files = [
        ("src/shared/ui/card/card.tsx", "export {};\n"),
        ("src/shared/ui/card/card.test.tsx", ""),
        ("src/shared/ui/card/card.stories.tsx", ""),
        ("src/shared/ui/card/badge.stories.tsx", ""),
    ];
    assert_eq!(
        check("colocated-files", NEXT, &files),
        vec![row("", "src/shared/ui/card/badge.stories.tsx", None)]
    );
}

#[test]
fn a_missing_pack_is_a_configuration_error_that_names_the_three() {
    let error = run_raw("layout", "{}", "{}", &[("src/main.tsx", "")])
        .expect_err("a run without a pack fails");
    assert!(
        error.message().contains("no pack")
            && error.message().contains("react-vite")
            && error.message().contains("nextjs")
            && error.message().contains("react-library"),
        "{}",
        error.message()
    );

    let unknown = run_raw(
        "layout",
        "{}",
        r#"{ "pack": "vue" }"#,
        &[("src/main.tsx", "")],
    );
    assert!(unknown.is_err(), "an unknown pack is refused");
}

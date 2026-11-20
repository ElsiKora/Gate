//! The rules that read paths only, run natively through the SDK against synthetic trees: layout,
//! file-names, typescript-sources, asset-placement and colocated-files, and the options errors of
//! the plugin.

mod support;

use support::{check, check_with, row, run_raw, without};

const NEXT: &str = "nextjs";
const VITE: &str = "react-vite";
const LIBRARY: &str = "react-library";

/// A Next.js app laid out as the standard says: the App Router in the root `app/`, FSD in `src/`.
const NEXT_APP: &[(&str, &str)] = &[
    ("next.config.ts", "export default {};\n"),
    ("middleware.ts", "export function middleware() {}\n"),
    ("instrumentation-client.ts", "export {};\n"),
    ("public/favicon.ico", ""),
    ("pages/README.md", "Keeps Next.js from reading src/pages.\n"),
    (
        "app/layout.tsx",
        "import \"@/app/styles/globals.css\";\nexport default function Layout() { return null; }\n",
    ),
    (
        "app/(shop)/invoices/[invoiceId]/page.tsx",
        "export { InvoicePage as default } from \"@/pages/invoice\";\n",
    ),
    (
        "app/@modal/(.)photo/page.tsx",
        "export default function Page() { return null; }\n",
    ),
    (
        "app/docs/[[...rest]]/page.tsx",
        "export default function Page() { return null; }\n",
    ),
    ("app/icon.png", ""),
    ("app/opengraph-image.png", ""),
    ("app/opengraph-image.alt.txt", "A synthetic image.\n"),
    (
        "app/global-not-found.tsx",
        "export default function GlobalNotFound() { return null; }\n",
    ),
    (
        "src/app/providers/providers.tsx",
        "export function Providers() { return null; }\n",
    ),
    ("src/app/store/store.ts", "export {};\n"),
    ("src/app/styles/globals.css", ""),
    ("src/pages/invoice/index.ts", "export {};\n"),
    (
        "src/pages/invoice/ui/invoice-page.tsx",
        "export function InvoicePage() { return null; }\n",
    ),
    ("src/widgets/header/index.ts", "export {};\n"),
    (
        "src/widgets/header/ui/header.tsx",
        "export function Header() { return null; }\n",
    ),
    ("src/features/auth/sign-in/index.ts", "export {};\n"),
    (
        "src/features/auth/sign-in/model/use-sign-in.ts",
        "export function useSignIn() {}\n",
    ),
    ("src/features/auth/sign-in/model/use-sign-in.test.ts", ""),
    ("src/features/auth/sign-out/index.ts", "export {};\n"),
    ("src/entities/user/index.ts", "export {};\n"),
    ("src/entities/user/index.server.ts", "export {};\n"),
    ("src/entities/user/@x/invoice.ts", "export {};\n"),
    ("src/entities/user/api/user-api.ts", "export {};\n"),
    ("src/entities/user/config/limits.ts", "export {};\n"),
    ("src/entities/user/lib/format.ts", "export {};\n"),
    ("src/entities/invoice/index.ts", "export {};\n"),
    ("src/shared/api/index.ts", "export {};\n"),
    ("src/shared/api/client.ts", "export {};\n"),
    ("src/shared/api/generated/models/UserDto.ts", "export {};\n"),
    (
        "src/shared/api/generated/petstore.schemas.ts",
        "export {};\n",
    ),
    ("src/shared/api/generated/model/user-dto.ts", "export {};\n"),
    ("src/shared/config/index.ts", "export {};\n"),
    ("src/shared/config/index.server.ts", "export {};\n"),
    ("src/shared/config/env.ts", "export {};\n"),
    ("src/shared/i18n/index.ts", "export {};\n"),
    ("src/shared/i18n/messages/en.json", "{}"),
    ("src/shared/routes/index.ts", "export {};\n"),
    ("src/shared/ui/logo-dark.svg", ""),
    (
        "src/shared/ui/button/button.tsx",
        "export function Button() { return null; }\n",
    ),
    ("src/shared/ui/button/button.module.css", ""),
    ("src/shared/lib/types.d.ts", "export {};\n"),
];

/// A Vite app laid out as the standard says.
const VITE_APP: &[(&str, &str)] = &[
    ("index.html", ""),
    ("vite.config.ts", "export default {};\n"),
    ("src/main.tsx", "export {};\n"),
    ("src/vite-env.d.ts", "export {};\n"),
    ("src/app/route-tree.gen.ts", "export {};\n"),
    ("src/app/entrypoint/router.ts", "export {};\n"),
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
    ("src/pages/home/index.ts", "export {};\n"),
    ("src/entities/invoice/index.ts", "export {};\n"),
];

/// A library laid out as the standard says.
const LIBRARY_TREE: &[(&str, &str)] = &[
    ("src/index.ts", "export {};\n"),
    (
        "src/ui/phone-input/phone-input.tsx",
        "export function PhoneInput() { return null; }\n",
    ),
    ("src/ui/flags.svg", ""),
    ("src/model/use-phone.ts", "export function usePhone() {}\n"),
    ("src/api/generated/models/UserDto.ts", "export {};\n"),
    ("src/api/generated/model/user-dto.ts", "export {};\n"),
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
        assert_eq!(check(rule, VITE, VITE_APP), vec![], "{rule}, react-vite");
        assert_eq!(
            check(rule, LIBRARY, LIBRARY_TREE),
            vec![],
            "{rule}, react-library"
        );
    }
}

#[test]
fn each_piece_of_the_standard_tree_is_required() {
    // Without the root pages/README.md, Next.js would read src/pages as the Pages Router.
    assert_eq!(
        check("layout", NEXT, &without(NEXT_APP, "pages/README.md")),
        vec![row("next-pages", "pages", None)]
    );
    // Without its index.ts, a shared segment other than ui and lib has no public API.
    assert_eq!(
        check(
            "layout",
            NEXT,
            &without(NEXT_APP, "src/shared/config/index.ts")
        ),
        vec![row("shared", "src/shared/config", None)]
    );
    // Without its index.ts, a slice in a group is still a slice that needs one; the group needs none.
    assert_eq!(
        check(
            "layout",
            NEXT,
            &without(NEXT_APP, "src/features/auth/sign-in/index.ts")
        ),
        vec![row("slice", "src/features/auth/sign-in", None)]
    );
    // index.server.ts is a public API in Next.js only.
    let mut vite = VITE_APP.to_vec();
    vite.push(("src/entities/invoice/index.server.ts", "export {};\n"));
    assert_eq!(
        check("layout", VITE, &vite),
        vec![row("segment", "src/entities/invoice/index.server.ts", None)]
    );
    // The generated route tree is the one file the app layer's root holds, in Vite only.
    let mut next = NEXT_APP.to_vec();
    next.push(("src/app/route-tree.gen.ts", "export {};\n"));
    assert_eq!(
        check("layout", NEXT, &next),
        vec![row("app", "src/app/route-tree.gen.ts", None)]
    );
}

#[test]
fn layout_reports_every_departure_once_at_its_folder() {
    let files = [
        ("pages/README.md", ""),
        ("pages/index.tsx", "export {};\n"),
        ("components/card.tsx", "export {};\n"),
        ("e2e/.keep", ""),
        ("src/modules/user/user.ts", "export {};\n"),
        ("src/features/UserCard/index.ts", "export {};\n"),
        ("src/features/no-index/ui/form.tsx", "export {};\n"),
        ("src/features/card/index.tsx", "export {};\n"),
        ("src/features/ui/button.tsx", "export {};\n"),
        ("src/features/like/index.ts", "export {};\n"),
        ("src/features/like/ui/lib/format.ts", "export {};\n"),
        ("src/features/like/@x/share.ts", "export {};\n"),
        ("src/features/auth/social/google/index.ts", "export {};\n"),
        (
            "src/features/auth/social/google/ui/button.tsx",
            "export {};\n",
        ),
        ("src/widgets/menu/index.ts", "export {};\n"),
        ("src/widgets/menu/side/index.ts", "export {};\n"),
        ("src/entities/user/index.ts", "export {};\n"),
        ("src/entities/user/user.ts", "export {};\n"),
        ("src/entities/user/helpers/format.ts", "export {};\n"),
        ("src/entities/user/@x/deep/order.ts", "export {};\n"),
        ("src/entities/loose.ts", "export {};\n"),
        ("src/app/ui/button.tsx", "export {};\n"),
        ("src/app/router/index.ts", "export {};\n"),
        ("src/app/providers.tsx", "export {};\n"),
        ("src/shared/user/index.ts", "export {};\n"),
        ("src/shared/config/env.ts", "export {};\n"),
        ("src/shared/assets/logo.svg", ""),
        ("src/shared/index.ts", "export {};\n"),
        ("src/router.tsx", "export {};\n"),
    ];
    assert_eq!(
        check("layout", NEXT, &files),
        vec![
            row("app", "src/app/providers.tsx", None),
            row("app", "src/app/router", None),
            row("app", "src/app/ui", None),
            row("cross-import", "src/entities/user/@x/deep/order.ts", None),
            row("cross-import", "src/features/like/@x", None),
            row("group", "src/features/auth/social", None),
            row("group", "src/widgets/menu/index.ts", None),
            row("layer", "src/modules", None),
            row("next-pages", "pages/index.tsx", None),
            row("reserved", "src/features/like/ui/lib", None),
            row("root-file", "src/router.tsx", None),
            row("segment", "src/entities/user/helpers", None),
            row("segment", "src/entities/user/user.ts", None),
            row("segment", "src/features/ui", None),
            row("shared", "src/shared/assets", None),
            row("shared", "src/shared/config", None),
            row("shared", "src/shared/index.ts", None),
            row("shared", "src/shared/user", None),
            row("slice", "src/entities/loose.ts", None),
            row("slice", "src/features/UserCard", None),
            row("slice", "src/features/card", None),
            row("slice", "src/features/card/index.tsx", None),
            row("slice", "src/features/no-index", None),
            row("source-root", "components", None),
        ]
    );
}

#[test]
fn layout_reports_a_file_of_a_group_s_own() {
    // A folder with child slices is a group even when it holds a file of its own; the file is
    // reported, not the slices as segments of a slice.
    let files = [
        ("pages/README.md", ""),
        ("src/features/auth/index.ts", "export {};\n"),
        ("src/features/auth/sign-in/index.ts", "export {};\n"),
    ];
    assert_eq!(
        check("layout", NEXT, &files),
        vec![row("group", "src/features/auth/index.ts", None)]
    );
}

#[test]
fn the_generated_client_is_reported_outside_its_folder() {
    // The names and the tree the standard tree passes inside src/shared/api/generated/.
    let files = [
        ("pages/README.md", ""),
        ("src/shared/api/index.ts", "export {};\n"),
        ("src/shared/api/models/UserDto.ts", "export {};\n"),
        ("src/shared/api/petstore.schemas.ts", "export {};\n"),
        ("src/shared/api/model/user-dto.ts", "export {};\n"),
    ];
    assert_eq!(
        check("file-names", NEXT, &files),
        vec![
            row("file", "src/shared/api/models/UserDto.ts", None),
            row("suffix", "src/shared/api/petstore.schemas.ts", None),
        ]
    );
    assert_eq!(
        check("layout", NEXT, &files),
        vec![row("reserved", "src/shared/api/model", None)]
    );
    let library = [("src/api/model/user-dto.ts", "export {};\n")];
    assert_eq!(
        check("layout", LIBRARY, &library),
        vec![row("reserved", "src/api/model", None)]
    );
}

#[test]
fn layout_holds_a_library_to_its_segments() {
    let files = [
        ("src/index.ts", "export {};\n"),
        ("src/components/card.tsx", "export {};\n"),
        ("src/assets/flags.svg", ""),
        ("src/main.tsx", "export {};\n"),
    ];
    assert_eq!(
        check("layout", LIBRARY, &files),
        vec![
            row("root-file", "src/main.tsx", None),
            row("shared", "src/assets", None),
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
        ("src/shared/config/index.server.ts", "export {};\n"),
        ("src/features/share/@x/user.ts", "export {};\n"),
        ("src/app/routes/[id].tsx", "export const Route = 1;\n"),
        ("src/app/routes/$id/index.tsx", "export const Route = 1;\n"),
        (
            "src/app/routes/$invoice_id.tsx",
            "export const Route = 1;\n",
        ),
        ("src/app/route-tree.gen.ts", "export {};\n"),
        ("src/shared/ui/card.lazy.tsx", "export {};\n"),
    ];
    assert_eq!(
        check("file-names", VITE, &files),
        vec![
            row("file", "src/app/routes/$invoice_id.tsx", None),
            row("file", "src/app/routes/[id].tsx", None),
            row("file", "src/shared/lib/userCard.ts", None),
            row("file", "src/shared/ui/UserCard.tsx", None),
            row("folder", "src/features/share/@x", None),
            row("folder", "src/shared/ui/Button", None),
            row("suffix", "src/shared/config/index.server.ts", None),
            row("suffix", "src/shared/lib/user.helper.ts", None),
            row("suffix", "src/shared/ui/card.lazy.tsx", None),
        ]
    );

    // `[id]` is Next.js syntax in the root app/ only; `$id` is not Next.js syntax.
    let next = [
        ("app/[id]/page.tsx", "export default 1;\n"),
        ("app/$id/page.tsx", "export default 1;\n"),
        ("app/UserCard/page.tsx", "export default 1;\n"),
        ("src/widgets/[id]/index.ts", "export {};\n"),
        ("src/entities/user/index.server.ts", "export {};\n"),
        ("app/blog/twitter-image.alt.txt", "A synthetic image.\n"),
        ("app/blog/hero.alt.txt", "A synthetic image.\n"),
        (
            "src/shared/ui/opengraph-image.alt.txt",
            "A synthetic image.\n",
        ),
    ];
    assert_eq!(
        check("file-names", NEXT, &next),
        vec![
            row("folder", "app/$id", None),
            row("folder", "app/UserCard", None),
            row("folder", "src/widgets/[id]", None),
            row("suffix", "app/blog/hero.alt.txt", None),
            row("suffix", "src/shared/ui/opengraph-image.alt.txt", None),
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
fn asset_placement_allows_ui_segments_public_and_next_metadata_images() {
    let files = [
        ("public/robots.txt", ""),
        ("public/hero.jpg", ""),
        ("src/shared/ui/logo.svg", ""),
        ("src/features/auth/ui/logo.png", ""),
        ("app/icon.png", ""),
        ("app/blog/opengraph-image.jpg", ""),
        ("app/blog/favicon.ico", ""),
        ("app/hero.jpg", ""),
        ("src/app/icon.png", ""),
        ("src/shared/assets/logo.svg", ""),
        ("src/assets/hero.jpg", ""),
        ("src/entities/user/model/avatar.PNG", ""),
    ];
    assert_eq!(
        check("asset-placement", NEXT, &files),
        vec![
            row("", "app/blog/favicon.ico", None),
            row("", "app/hero.jpg", None),
            row("", "src/app/icon.png", None),
            row("", "src/assets/hero.jpg", None),
            row("", "src/entities/user/model/avatar.PNG", None),
            row("", "src/shared/assets/logo.svg", None),
        ]
    );
    assert_eq!(
        check_with(
            "asset-placement",
            NEXT,
            r#"{ "extensions": ["svg"] }"#,
            &[("src/features/auth/model/logo.png", "")]
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

#[test]
fn an_option_of_the_wrong_type_is_a_configuration_error() {
    let wrong = run_raw(
        "asset-placement",
        r#"{ "extensions": "svg" }"#,
        r#"{ "pack": "nextjs" }"#,
        &[("src/shared/ui/logo.svg", "")],
    );
    assert!(wrong.is_err(), "a string where a list belongs is refused");
}

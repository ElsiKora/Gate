//! The rules that read the code, run natively through the SDK against synthetic trees: imports,
//! names, state, the network, the environment, text and messages, routes, the client boundary and
//! styles.

mod support;

use support::{check, check_with, messages, row, run_raw};

const NEXT: &str = "nextjs";
const VITE: &str = "react-vite";
const LIBRARY: &str = "react-library";

#[test]
fn import_direction_lets_layers_point_down_only() {
    let files = [
        (
            "src/app/providers.tsx",
            "import { Header } from \"@/widgets/header\";\n",
        ),
        (
            "src/widgets/header/index.ts",
            "import { signIn } from \"@/features/sign-in\";\nimport { user } from \"@/entities/user\";\nimport { client } from \"@/shared/api\";\n",
        ),
        (
            "src/features/sign-in/index.ts",
            "export const signIn = 1;\n",
        ),
        (
            "src/features/sign-up/index.ts",
            "import { signIn } from \"@/features/sign-in\";\n",
        ),
        (
            "src/entities/user/index.ts",
            "export const user = 1;\nimport type { SignIn } from \"@/features/sign-in\";\n",
        ),
        (
            "src/shared/api/index.ts",
            "export const client = 1;\nimport { user } from \"@/entities/user\";\n",
        ),
    ];
    assert_eq!(
        check("import-direction", NEXT, &files),
        vec![
            row("cross-slice", "src/features/sign-up/index.ts", Some(1)),
            row("layer", "src/entities/user/index.ts", Some(2)),
            row("layer", "src/shared/api/index.ts", Some(2)),
        ]
    );
    assert_eq!(check("import-direction", LIBRARY, &files), vec![]);
}

#[test]
fn import_paths_keeps_one_alias_and_relative_imports_inside_a_unit() {
    let files = [
        (
            "src/features/auth/index.ts",
            "export { Form } from \"./ui/form\";\n",
        ),
        (
            "src/features/auth/ui/form.tsx",
            "import { useAuth } from \"../model/use-auth\";\nimport { user } from \"@/entities/user\";\nimport { useAuth as again } from \"@/features/auth/model/use-auth\";\nimport { user as other } from \"../../../entities/user\";\nimport { Icon } from \"@icons/x\";\nimport { useState } from \"react\";\nexport function Form() { return null; }\n",
        ),
        (
            "src/features/auth/model/use-auth.ts",
            "export function useAuth() {}\n",
        ),
        ("src/entities/user/index.ts", "export const user = 1;\n"),
        ("src/shared/icons/x.ts", "export const Icon = 1;\n"),
        (
            "tsconfig.json",
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./src/*"], "@icons/*": ["./src/shared/icons/*"] } } }"#,
        ),
    ];
    assert_eq!(
        check("import-paths", NEXT, &files),
        vec![
            row("absolute-inside", "src/features/auth/ui/form.tsx", Some(3)),
            row("alias", "src/features/auth/ui/form.tsx", Some(5)),
            row("relative-exit", "src/features/auth/ui/form.tsx", Some(4)),
        ]
    );
}

#[test]
fn component_names_ties_a_ui_file_to_its_component() {
    let files = [
        (
            "src/entities/user/ui/user-card.tsx",
            "export type UserCardProps = {};\nexport function UserCard() { return null; }\nexport function UserCardHeader() { return null; }\nexport const size = 1;\n",
        ),
        (
            "src/entities/user/ui/user-avatar.tsx",
            "export function Avatar() { return null; }\nexport default Avatar;\n",
        ),
        (
            "src/entities/user/ui/user-card.stories.tsx",
            "export default {};\n",
        ),
        (
            "src/entities/user/model/user.tsx",
            "export function Anything() { return null; }\n",
        ),
        (
            "src/app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
    ];
    assert_eq!(
        check("component-names", NEXT, &files),
        vec![
            row(
                "default-export",
                "src/entities/user/ui/user-avatar.tsx",
                Some(2)
            ),
            row("extra", "src/entities/user/ui/user-avatar.tsx", Some(1)),
            row("name", "src/entities/user/ui/user-avatar.tsx", None),
        ]
    );
}

#[test]
fn hook_names_ties_a_hook_to_its_file_and_its_segment() {
    let files = [
        (
            "src/entities/balance/model/use-balance.ts",
            "export function useBalance() {}\n",
        ),
        (
            "src/shared/lib/use-media-query.ts",
            "export const useMediaQuery = () => {};\n",
        ),
        (
            "src/entities/balance/model/balance.ts",
            "export function useBalance() {}\n",
        ),
        (
            "src/entities/balance/model/use-total.ts",
            "export function getTotal() {}\n",
        ),
        (
            "src/entities/balance/ui/use-toggle.ts",
            "export function useToggle() {}\n",
        ),
        (
            "src/entities/balance/index.ts",
            "export { useBalance } from \"./model/use-balance\";\n",
        ),
    ];
    assert_eq!(
        check("hook-names", NEXT, &files),
        vec![
            row("export", "src/entities/balance/model/use-total.ts", None),
            row("file", "src/entities/balance/model/balance.ts", Some(1)),
            row("placement", "src/entities/balance/ui/use-toggle.ts", None),
        ]
    );
}

#[test]
fn state_placement_finds_a_store_through_its_import_binding() {
    let files = [
        (
            "src/entities/cart/model/use-cart-store.ts",
            "import { create } from \"zustand\";\nexport const useCartStore = create(() => ({}));\n",
        ),
        (
            "src/app/store.ts",
            "import { configureStore } from \"@reduxjs/toolkit\";\nexport const store = configureStore({ reducer: {} });\n",
        ),
        (
            "src/entities/cart/ui/cart.tsx",
            "import { create as make } from \"zustand\";\nconst useLocal = make(() => ({}));\n",
        ),
        (
            "src/shared/lib/store.ts",
            "import * as toolkit from \"@reduxjs/toolkit\";\nexport const slice = toolkit.createSlice({});\n",
        ),
        (
            "src/shared/lib/local.ts",
            "function create() {}\ncreate();\n",
        ),
    ];
    assert_eq!(
        check("state-placement", NEXT, &files),
        vec![
            row("", "src/entities/cart/ui/cart.tsx", Some(2)),
            row("", "src/shared/lib/store.ts", Some(2)),
        ]
    );
}

#[test]
fn api_boundary_keeps_the_network_in_the_api_segment() {
    let files = [
        (
            "src/shared/api/client.ts",
            "import axios from \"axios\";\nexport const client = axios.create();\nfetch(\"/x\");\n",
        ),
        (
            "src/shared/api/index.ts",
            "export { client } from \"./client\";\n",
        ),
        (
            "src/entities/user/api/user-api.ts",
            "import { client } from \"@/shared/api\";\nexport const getUser = () => client.get(\"/user\");\n",
        ),
        (
            "src/entities/user/ui/user-card.tsx",
            "import { client } from \"@/shared/api\";\nfetch(\"/a\");\nwindow.fetch(\"/b\");\nglobalThis.fetch(\"/c\");\n",
        ),
        (
            "src/entities/user/model/use-user.ts",
            "import { get } from \"axios\";\nimport type { AxiosError } from \"axios\";\nfunction fetch() {}\nfetch();\n",
        ),
    ];
    assert_eq!(
        check("api-boundary", NEXT, &files),
        vec![
            row(
                "client-import",
                "src/entities/user/ui/user-card.tsx",
                Some(1)
            ),
            row("raw-call", "src/entities/user/model/use-user.ts", Some(1)),
            row("raw-call", "src/entities/user/ui/user-card.tsx", Some(2)),
            row("raw-call", "src/entities/user/ui/user-card.tsx", Some(3)),
            row("raw-call", "src/entities/user/ui/user-card.tsx", Some(4)),
        ]
    );
}

#[test]
fn env_access_keeps_literal_public_reads_in_the_environment_module() {
    let vite = [
        (
            "src/shared/config/env.ts",
            "export const api = import.meta.env.VITE_API_URL;\nexport const mode = import.meta.env.MODE;\nexport const secret = import.meta.env.SECRET;\nconst key = \"X\";\nexport const dynamic = import.meta.env[key];\nconst { VITE_A } = import.meta.env;\nexport const node = process.env.VITE_B;\n",
        ),
        (
            "src/entities/user/ui/user-card.tsx",
            "export const url = import.meta.env.VITE_API_URL;\n",
        ),
    ];
    assert_eq!(
        check("env-access", VITE, &vite),
        vec![
            row("dynamic", "src/shared/config/env.ts", Some(5)),
            row("dynamic", "src/shared/config/env.ts", Some(6)),
            row("outside", "src/entities/user/ui/user-card.tsx", Some(1)),
            row("prefix", "src/shared/config/env.ts", Some(3)),
            row("prefix", "src/shared/config/env.ts", Some(7)),
        ]
    );

    let next = [
        (
            "src/shared/config/env.ts",
            "export const api = process.env.NEXT_PUBLIC_API_URL;\nexport const secret = process.env.DATABASE_URL;\n",
        ),
        (
            "src/shared/config/server-env.ts",
            "export const secret = process.env.DATABASE_URL;\n",
        ),
    ];
    assert_eq!(
        check("env-access", NEXT, &next),
        vec![
            row("prefix", "src/shared/config/env.ts", Some(2)),
            row("server-guard", "src/shared/config/server-env.ts", None),
        ]
    );
    let guarded = [(
        "src/shared/config/server-env.ts",
        "import \"server-only\";\nexport const secret = process.env.DATABASE_URL;\n",
    )];
    assert_eq!(check("env-access", NEXT, &guarded), vec![]);

    let library = [(
        "src/lib/config.ts",
        "export const x = import.meta.env.VITE_X;\n",
    )];
    assert_eq!(
        check("env-access", LIBRARY, &library),
        vec![row("outside", "src/lib/config.ts", Some(1))]
    );
}

#[test]
fn literal_text_reports_text_and_user_visible_attributes() {
    let files = [(
        "src/widgets/header/ui/header.tsx",
        "export function Header({ t }: { t: (key: string) => string }) {\n  return (\n    <>\n      <h1>Welcome back</h1>\n      <p>{t(\"title\")}</p>\n      <span>{\"Hello\"}</span>\n      <span> 42 %</span>\n      <input placeholder=\"Search\" className=\"input wide\" />\n      <img alt={t(\"logo\")} />\n    </>\n  );\n}\n",
    )];
    assert_eq!(
        check("literal-text", NEXT, &files),
        vec![
            row("attribute", "src/widgets/header/ui/header.tsx", Some(8)),
            row("text", "src/widgets/header/ui/header.tsx", Some(4)),
            row("text", "src/widgets/header/ui/header.tsx", Some(6)),
        ]
    );
    assert_eq!(
        check_with(
            "literal-text",
            NEXT,
            r#"{ "ignore": "^(Welcome back|Hello|Search)$" }"#,
            &files
        ),
        vec![]
    );
    assert!(
        run_raw(
            "literal-text",
            r#"{ "ignore": "(" }"#,
            r#"{ "pack": "nextjs" }"#,
            &files
        )
        .expect_err("a broken pattern fails")
        .message()
        .contains("no regular expression")
    );
}

#[test]
fn messages_compares_the_locales_and_the_keys_the_code_uses() {
    let next = [
        (
            "src/shared/i18n/messages/en.json",
            r#"{ "home": { "title": "Home", "lead": "Welcome" }, "empty": "" }"#,
        ),
        (
            "src/shared/i18n/messages/de.json",
            r#"{ "home": { "title": "Start", "extra": "Mehr" }, "empty": "x" }"#,
        ),
        (
            "src/widgets/home/ui/home.tsx",
            "import { useTranslations } from \"next-intl\";\nexport function Home() {\n  const t = useTranslations(\"home\");\n  return <>{t(\"title\")}{t(\"missing\")}{t.rich(\"lead\")}</>;\n}\n",
        ),
        (
            "src/app/page.tsx",
            "import { getTranslations } from \"next-intl/server\";\nexport default async function Page() {\n  const t = await getTranslations({ namespace: \"home\" });\n  return t(\"gone\");\n}\n",
        ),
    ];
    assert_eq!(
        check("messages", NEXT, &next),
        vec![
            row("empty", "src/shared/i18n/messages/en.json", None),
            row("locale-keys", "src/shared/i18n/messages/de.json", None),
            row("locale-keys", "src/shared/i18n/messages/de.json", None),
            row("unknown-key", "src/app/page.tsx", Some(4)),
            row("unknown-key", "src/widgets/home/ui/home.tsx", Some(4)),
        ]
    );

    let vite = [
        (
            "src/shared/i18n/messages/en.json",
            r#"{ "a": { "b": "B" } }"#,
        ),
        (
            "src/widgets/home/ui/home.tsx",
            "import { useTranslation } from \"react-i18next\";\nexport function Home() {\n  const { t } = useTranslation(undefined, { keyPrefix: \"a\" });\n  return <>{t(\"b\")}{t(\"c\")}</>;\n}\n",
        ),
    ];
    assert_eq!(
        messages("messages", VITE, &vite),
        vec!["src/widgets/home/ui/home.tsx shows the message a.c, which src/shared/i18n/messages/en.json lacks".to_owned()]
    );
}

#[test]
fn route_files_holds_each_router_to_its_files_and_exports() {
    let next = [
        (
            "src/app/providers.tsx",
            "export function Providers() { return null; }\n",
        ),
        (
            "src/app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
        (
            "src/app/(dashboard)/table.tsx",
            "export function Table() { return null; }\n",
        ),
        (
            "src/app/(dashboard)/layout.tsx",
            "export function Layout() { return null; }\n",
        ),
        (
            "src/app/api/health/route.ts",
            "export async function GET() { return new Response(); }\n",
        ),
        ("src/app/api/broken/route.ts", "export const handler = 1;\n"),
    ];
    assert_eq!(
        check("route-files", NEXT, &next),
        vec![
            row("export", "src/app/(dashboard)/layout.tsx", None),
            row("export", "src/app/api/broken/route.ts", None),
            row("foreign", "src/app/(dashboard)/table.tsx", None),
        ]
    );

    let vite = [
        (
            "src/app/routes/__root.tsx",
            "export const Route = createRootRoute();\n",
        ),
        (
            "src/app/routes/index.tsx",
            "export const Route = createFileRoute(\"/\")();\n",
        ),
        (
            "src/app/routes/use-session.ts",
            "export function useSession() {}\n",
        ),
        ("src/app/routes/index.module.css", ""),
    ];
    assert_eq!(
        check("route-files", VITE, &vite),
        vec![
            row("export", "src/app/routes/use-session.ts", None),
            row("foreign", "src/app/routes/index.module.css", None),
        ]
    );
}

#[test]
fn client_boundary_keeps_route_files_on_the_server() {
    let files = [
        (
            "src/app/providers.tsx",
            "\"use client\";\nexport function Providers() { return null; }\n",
        ),
        (
            "src/app/page.tsx",
            "\"use client\";\nexport default function Page() { return null; }\n",
        ),
        (
            "src/app/blog/layout.tsx",
            "\"use client\";\nexport default function Layout() { return null; }\n",
        ),
        (
            "src/features/like/ui/like-button.tsx",
            "\"use client\";\nexport function LikeButton() { return null; }\n",
        ),
        (
            "src/shared/lib/format.ts",
            "\"use client\";\nexport const format = 1;\n",
        ),
    ];
    assert_eq!(
        check("client-boundary", NEXT, &files),
        vec![
            row("placement", "src/shared/lib/format.ts", None),
            row("route-file", "src/app/blog/layout.tsx", None),
            row("route-file", "src/app/page.tsx", None),
        ]
    );
    assert_eq!(check("client-boundary", VITE, &files), vec![]);
}

#[test]
fn style_files_keeps_global_styles_in_the_app_layer() {
    let files = [
        ("src/app/globals.css", ""),
        ("src/app/extra.css", ""),
        (
            "src/app/layout.tsx",
            "import \"./globals.css\";\nimport \"./extra.css\";\nexport default function Layout() { return null; }\n",
        ),
        (
            "src/shared/ui/card/card.tsx",
            "import styles from \"./card.module.css\";\nimport \"./card.css\";\nexport function Card() { return null; }\n",
        ),
        ("src/shared/ui/card/card.module.css", ""),
        ("src/shared/ui/card/card.css", ""),
        ("src/shared/ui/card/badge.module.css", ""),
    ];
    assert_eq!(
        check("style-files", NEXT, &files),
        vec![
            row("global", "src/app/layout.tsx", Some(2)),
            row("global", "src/shared/ui/card/card.css", None),
            row("import", "src/shared/ui/card/card.tsx", Some(2)),
            row("orphan", "src/shared/ui/card/badge.module.css", None),
        ]
    );
}

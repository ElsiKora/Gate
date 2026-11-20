use crate::fixture::{Fixture, message};

const RULE: &str = "formatter";

const STANDARD: &str = "{\n  \"$schema\": \"https://json.schemastore.org/prettierrc\",\n  \"printWidth\": 120,\n  \"proseWrap\": \"preserve\",\n  \"trailingComma\": \"all\"\n}\n";

fn run(config: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![("package.json", "{}"), (".prettierrc.json", config)];
    files.extend_from_slice(extra);
    Fixture::new("service", &files).lines(RULE)
}

#[test]
fn the_standard_root_configuration_passes() {
    assert_eq!(
        run(STANDARD, &[(".prettierignore", "dist\n")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_setting_other_than_the_standard_is_reported_at_its_key() {
    let changed = STANDARD.replace("120", "100");
    assert_eq!(
        run(&changed, &[]),
        ["settings .prettierrc.json:3 printWidth"]
    );
    let fixture = Fixture::new(
        "service",
        &[("package.json", "{}"), (".prettierrc.json", &changed)],
    );
    assert_eq!(
        message(&fixture, RULE),
        ".prettierrc.json sets printWidth to 100; the standard is 120"
    );
    let other = "{\n  \"printWidth\": 120,\n  \"tabWidth\": 1,\n  \"trailingComma\": \"all\"\n}\n";
    assert_eq!(
        run(other, &[]),
        [
            "settings .prettierrc.json proseWrap",
            "settings .prettierrc.json:3 tabWidth"
        ]
    );
}

#[test]
fn another_form_or_a_nested_configuration_is_reported() {
    let extra = [
        ("prettier.config.mjs", "export default {};\n"),
        ("apps/a/.prettierrc", "{}"),
        ("apps/a/.prettierrc.json", "{}"),
        (
            "apps/b/package.json",
            "{\n  \"name\": \"b\",\n  \"prettier\": {}\n}\n",
        ),
    ];
    assert_eq!(
        run(STANDARD, &extra),
        [
            "nested apps/a/.prettierrc .prettierrc",
            "nested apps/a/.prettierrc.json .prettierrc.json",
            "form prettier.config.mjs prettier.config.mjs",
            "nested apps/b/package.json:3 prettier"
        ]
    );
}

#[test]
fn a_repository_without_the_root_configuration_is_reported() {
    let files = [("package.json", "{\n  \"prettier\": {}\n}\n")];
    assert_eq!(
        Fixture::new("service", &files).lines(RULE),
        [
            "form package.json:2 prettier",
            "form .prettierrc.json .prettierrc.json"
        ]
    );
}

#[test]
fn an_editorconfig_that_changes_prettier_s_output_is_reported() {
    let agrees = "root = true\n\n[*]\nindent_style = space\nindent_size = 2\nend_of_line = lf\nmax_line_length = 100\nquote_type = double\n\n[Makefile]\nindent_style = tab\n";
    assert_eq!(
        run(STANDARD, &[(".editorconfig", agrees)]),
        Vec::<String>::new()
    );
    let disagrees = "\u{feff}root = true\n\n[*]\nindent_style = Tab\nend_of_line = crlf\nquote_type = single\n\n[*.{md,yml}]\nindent_size = tab\n\n[*.json]\ntab_width = 8\n\n[*.css]\nindent_style = space\nindent_size = 4\ntab_width = 2\n\n[*.ts]\nindent_style = space\nindent_size = 2\ntab_width = 8\n\n[*.py]\nindent_size = 4\n";
    assert_eq!(
        run(STANDARD, &[("apps/a/.editorconfig", disagrees)]),
        [
            "editorconfig apps/a/.editorconfig:4 [*]indent_style",
            "editorconfig apps/a/.editorconfig:5 [*]end_of_line",
            "editorconfig apps/a/.editorconfig:6 [*]quote_type",
            "editorconfig apps/a/.editorconfig:9 [*.{md,yml}]indent_size",
            "editorconfig apps/a/.editorconfig:12 [*.json]tab_width",
            "editorconfig apps/a/.editorconfig:16 [*.css]indent_size"
        ]
    );
    let fixture = Fixture::new(
        "service",
        &[
            ("package.json", "{}"),
            (".prettierrc.json", STANDARD),
            (".editorconfig", "[*]\nindent_size = 4\n"),
        ],
    );
    assert_eq!(
        message(&fixture, RULE),
        ".editorconfig sets indent_size = 4 for [*], and Prettier reads it; the standard formats with 2"
    );
}

#[test]
fn an_editorconfig_key_is_read_only_where_prettierrc_leaves_its_option_unset() {
    let wide = "[*]\nmax_line_length = 100\nindent_size = 4\n";
    let no_width = STANDARD.replace("  \"printWidth\": 120,\n", "");
    assert_eq!(
        run(&no_width, &[(".editorconfig", wide)]),
        [
            "settings .prettierrc.json printWidth",
            "editorconfig .editorconfig:2 [*]max_line_length",
            "editorconfig .editorconfig:3 [*]indent_size"
        ]
    );
    let with_width = STANDARD.replace(
        "\"trailingComma\": \"all\"",
        "\"trailingComma\": \"all\",\n  \"tabWidth\": 4",
    );
    assert_eq!(
        run(&with_width, &[(".editorconfig", wide)]),
        ["settings .prettierrc.json:6 tabWidth"]
    );
}

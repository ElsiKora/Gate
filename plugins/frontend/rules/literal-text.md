# `frontend/literal-text`

User-visible text goes through the translation layer.

## What it checks and why

Text in code cannot be translated or reviewed in one place, and every app goes through the
translation layer, including an app with one language today. In a `.tsx` file (tests and stories not
read):

- `text`: JSX text, or a string literal as a JSX child (`{"Hello"}`), with a letter in it.
- `attribute`: a string literal with a letter as the value of an attribute of `attributes`.

Text the `ignore` pattern matches (as a whole, trimmed) is not language and is not reported. Target
`<path>#<text>`, the text trimmed to 40 characters.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `text`, `attribute`. Findings
name their path as the target unless the text above names another.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `attributes` | list of strings | `alt`, `aria-label`, `aria-description`, `label`, `placeholder`, `title` | The attributes a user reads or hears. |
| `ignore` | regular expression | `^[\d\s.,:;%/()+-]*$` | Text that is not language. A pattern that is no regular expression is a configuration error. |

Every rule also reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/literal-text src/widgets/header/ui/header.tsx:4
  src/widgets/header/ui/header.tsx shows the literal text "Welcome back"
```

## How to fix it

Put the text into the messages of every locale, and show it through the translation function (`t("header.welcome")`).

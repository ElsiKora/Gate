# `frontend/messages`

Every locale has the same messages, none empty, and the code uses only messages that exist.

## What it checks and why

Messages are `src/shared/i18n/messages/<locale>.json` (a library's `src/i18n/messages/`), one file per
locale, nested objects of strings; a key is the dotted path of a leaf.

- `locale-keys`: a key one locale has and the base locale lacks, or the other way round, reported at
  the locale that differs; a messages folder without the base locale's file.
- `empty`: a message that is empty or white space.
- `unknown-key`: a literal key the code passes to its translation function that the base locale
  lacks. The function is bound as the pack's library binds it: in `nextjs`, next-intl's
  `const t = useTranslations("ns")` or `await getTranslations("ns" | { namespace })`, with `t(…)`,
  `t.rich(…)`, `t.markup(…)` and `t.raw(…)`; in the other packs, react-i18next's
  `const { t } = useTranslation(ns, { keyPrefix })`. A key that is not a literal is not read.

A file that is not JSON stops the run with an error that names it. Target `<path>#<key>`.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `locale-keys`, `empty`, `unknown-key`. Findings
name their path as the target unless the text above names another.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `base-locale` | string | `"en"` | The locale every other locale and every key in the code is checked against. |

Every rule also reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/messages src/widgets/home/ui/home.tsx:4
  src/widgets/home/ui/home.tsx shows the message home.missing, which src/shared/i18n/messages/en.json lacks
```

## How to fix it

Add the message to every locale, translate the missing ones, write the empty ones, or correct the key in the code.

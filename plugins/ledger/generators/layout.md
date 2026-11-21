# `ledger/layout`

Seeds the ledger manifest and the shared ledger files of an empty project, at the paths the project
configures.

## What it writes

Every file is a seed: `gate sync` writes it only where nothing exists, never overwrites it, and
`gate sync --check` never reports it. The ledger is the project's from the moment it is written.
A file that is tracked is never seeded, so the generator runs next to `ledger/decision-counts` and
`ledger/review-progress`, which write tracked ledger files, and `gate sync` writes each path once.

- **The manifest**, at `manifest`, in the shape [`ledger/manifest`](../rules/manifest.md) reads:
  `canonicalRoot` is the deepest directory that holds the five shared files, `canonicalFiles`
  lists them below that root, `idFamilies` is `{}` and `forbiddenReferences` is `[]`. No family is
  seeded, because `ledger/manifest#id-family` reports a family whose pattern matches nothing; add
  one when the ledger has records of it.
- **The five shared files** of `[plugins.ledger] options` (`decisions`, `implementation`,
  `verification`, `follow-ups`, `blockers`), each with a title. The decision index also gets the
  managed block of [`ledger/decision-counts`](decision-counts.md), holding the counts of an empty
  index as that generator writes them, and the header of its table,
  `| ID | Status | Title | Canonical file |`. A path two options name is seeded once.
- **The files a tracked manifest lists.** When `manifest` is tracked, each of its `canonicalFiles`
  is seeded too, titled by its name (`risk-register.md` is `# Risk register`). A path that leaves
  the project is skipped. A manifest that is not that shape is left to `ledger/manifest`.

No seed carries a marker line. The seeded ledger is structurally complete: every file the rules
locate exists, and the manifest passes `ledger/manifest`. What is still missing asks the person to
write something only they can write: the ADR coverage table of `ledger/decisions`, the catalog of
`ledger/reuses` and the plan of `parity`, each as `#input` of the rule that reads it, and the first
implementation record and first production blocker record, as `ledger/records#record-shape` keyed
`implementation record#none` and `production blocker record#none`. No record is seeded: a generic
one would be fake content. The generator writes no file under a glob any generator owns,
and owns none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `manifest` | string | `"docs/ledger/manifest.json"` | The ledger manifest, relative to the project, in normal form. |

`generate` receives the generator's own options and `[plugins.ledger] options`, so the shared files
follow the project's paths without repeating them. A project whose `ledger/manifest` reads another
manifest sets `manifest` here too.

```toml
[generate]
"ledger/layout" = {}
```

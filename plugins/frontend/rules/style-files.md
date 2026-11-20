# `frontend/style-files`

Global styles live in one place; every other stylesheet is a module beside its component.

## What it checks and why

Global styles stay in one place, and component styles move with their component. A stylesheet is a
`.css`, `.scss`, `.sass` or `.less` file under `src/` (and Next.js's root `app/`); a CSS module carries
the dotted part `module`.

- `global`: a stylesheet that is not a CSS module, outside `src/app/styles/` (a library's `src/ui/`);
  and an import of a second global stylesheet from the code, beside the first one imported (files in
  path order).
- `orphan`: a CSS module `<name>.module.<ext>` without `<name>.tsx` in its folder.
- `import`: an import of a stylesheet that is not a CSS module from outside the app layer, the files
  directly in `src/` and Next.js's root `app/`. Target `<path>#<specifier>`.

Which styling tool a project uses is `frontend/packages`'s concern, not this rule's.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `global`, `orphan`, `import`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/style-files src/shared/ui/card/card.tsx:2
  src/shared/ui/card/card.tsx imports the global stylesheet ./card.css outside the app layer
```

## How to fix it

Rename a component's stylesheet to `<component>.module.css` beside `<component>.tsx`, keep global styles in `src/app/styles/` and import them once from the app; import further global sheets from the first.

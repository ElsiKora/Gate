# `frontend/component-names`

A component file exports by name the component its name gives.

## What it checks and why

A file's name finds its component, and the component finds its file. A `.tsx` file of a `ui`
segment (of a slice, of `shared/ui`, of a library's `src/ui`) named `user-card.tsx`:

- `name`: does not export `UserCard` as a value (target `<path>#UserCard`). A file with an
  `export *` may export it and is not reported.
- `extra`: exports another component whose name does not start with `UserCard` (`UserCardHeader`
  passes, `Avatar` does not). A component is a function, a class, or a `const` bound to a function, a
  class or `memo(…)` or `forwardRef(…)` of one, whose name starts with a capital letter and is not all
  capitals. Constants (`MAX_SIZE`, an object `Theme`), enums and types are not read.
- `default-export`: has a default export; a component is exported by its name.

`index.tsx` and files with a dotted part (`.stories`, `.test`) are not read. Next.js route files
are not in a `ui` segment and keep their default exports.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `name`, `extra`, `default-export`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/component-names src/entities/user/ui/user-avatar.tsx
  src/entities/user/ui/user-avatar.tsx does not export the component UserAvatar, which its name gives
```

## How to fix it

Export the component as the file's name in `PascalCase`, move other components to files of their own name, and replace a default export with a named one.

# `cdk/file-names`

Every file below `lib/` is `<name>.<role>.ts` in kebab case, with a role of the standard, in the
place of its role.

## What it checks and why

The role in a file's name says what the file declares, so a reader knows a file before opening it and
the other rules know what to expect of it. Each file has one finding at most, the first that applies
in this order:

- `case`: a directory or a word of the file name is not kebab case.
- `bare-role`: the file is named only by roles, such as `function.ts` or `utility.function.ts`.
- `role`: the name does not end with a role of the standard (`stack`, `construct`, `deployment`,
  `aspect`, `synthesizer`, `interface`, `type`, `enum`, `constant`, `function`, `utility`,
  `configuration`, `validation`, `error`).
- `role-place`: a `stack`, `construct` or `deployment` file outside `lib/stack/`, `lib/construct/`
  or `lib/deployment/`, or a file in a role directory (`interface/`, `type/`, `enum/`, `constant/`,
  `function/`, `utility/`) with another role.

`index.ts` passes after the case check.

Scope: project. Reads history: no. Aspects: `case`, `role`, `role-place`, `bare-role`.

## Options

None.

## Example finding

```text
error cdk/file-names#role lib/naming/stack-name.helper.ts
  lib/naming/stack-name.helper.ts does not end with a role of the standard (stack, construct, deployment, aspect, synthesizer, interface, type, enum, constant, function, utility, configuration, validation, error)
```

## How to fix it

Rename the file to `<name>.<role>.ts` in kebab case, with the role of what it declares, such as
`create-stack-name.function.ts`, and move it below the directory of its role.

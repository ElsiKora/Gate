# `cdk/properties`

A construct takes `(scope, id, properties)`, typed `I<Class>Properties` from the `interface/` folder
beside it.

## What it checks and why

One shape for every construct makes each one read the same, and the properties interface beside the
class is where a reader looks for what it takes. The rule judges each class of `lib/` that extends
`Stack` or `Stage` of `aws-cdk-lib` or `Construct` of `constructs`, and each interface of `lib/`.

- `parameter`: the constructor's third parameter is not named `properties`.
- `interface`: the third parameter is not typed `I<Class>Properties` imported from
  `<folder>/interface/properties.interface.ts` or `<folder>/interface/index.ts`, where the folder is
  the class's; or an interface is named `…Props`.

Scope: project. Reads history: no. Aspects: `parameter`, `interface`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/properties#parameter lib/stack/network/network.stack.ts:9
  lib/stack/network/network.stack.ts: the third parameter of NetworkStack's constructor is not named properties
```

## How to fix it

Declare `I<Class>Properties` in `interface/properties.interface.ts` beside the class, re-export it from
`interface/index.ts`, name the parameter `properties` and type it with the interface.

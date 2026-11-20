# `agents/rule-enforcement`

Every rule declares a valid enforcement for its class, and every rule norm in the configuration
names a rule enforced by Gate.

## What it checks and why

The metadata line says how a rule is held true. A claim that cannot be checked (a test that is not
tracked, a tsconfig that does not set the option, a gate rule that no configured rule reports) is a
rule nobody enforces. By class, the detail must be:

| Class | Detail | Also checked |
| --- | --- | --- |
| `gate` | none | some configured rule carries the rule id as its norm |
| `eslint` | `<rule-id> <files-glob> <root>` | the glob is valid; the root is one of `eslint-roots` |
| `typecheck` | `<compiler-option> <tsconfig>` | the tsconfig is tracked and contains `"<compiler-option>"` |
| `test` | `<path of the owning test>` | the test is tracked |
| `review`, `advisory` | `-- <reason>` | the reason is not blank |

A `removed` rule is not checked. In the other direction, every norm of the configuration that
starts with `RULE-` must name a rule that a rule file defines with class `gate`; that finding has no
location, since the configuration line is the core's.

`#input`: `<root>/rules` holds no rule file (`*.md` directly in it), reported once at the directory
and keyed by it; the rule checks nothing else until it does. A project without its agent docs fails
the rule: create a rule file, point `root` at the agent docs, or turn the input off with
`levels = { input = "off" }` and a `decision`.

Scope: project. Reads history: no. Aspects: `input`; the other findings have no aspect.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory of the agent instructions, relative to the project. |
| `eslint-roots` | array of string | `["root"]` | The roots an `eslint` detail may end with. At least one. |

```toml
[rules]
"agents/rule-enforcement" = { level = "error", options = { eslint-roots = ["root", "infra"] } }
```

## Example finding

`docs/agents/rules/api.md:10` reads `Enforcement: test tests/api.test.ts · Status: stable`, and
`tests/api.test.ts` is not tracked:

```text
error agents/rule-enforcement docs/agents/rules/api.md:10
  the owning test tests/api.test.ts is not tracked
```

The finding's target is the rule id, or the norm for a configuration finding.

## How to fix it

Write the detail its class needs, add the file it names, or map the norm on a configured rule.

# `nestjs/asset-storage`

Public assets are admitted centrally and stored only through the S3 adapter.

## What it checks and why

Uploaded images are untrusted bytes that end up public. One central resource admits them, checks their
type and checksum and stores them through one adapter; business modules keep only a key. A feature that
admits bytes itself, persists a URL, or reaches storage directly skips those checks, and an image URL
stored in a row breaks when the bucket or CDN moves.

Files are the `.ts` files below a folder, compared case-sensitively, declaration files included, in
every folder: the rule skips none of `[languages.typescript] skip-folders`. The domain plane is `layout.domain-folder` below `layout.modules-root`; the application
planes are the other `layout.module-planes`.

- `#persisted-image-key`: a property of a top-level class in the domain plane, decorated with one of
  `layout.typeorm-column-decorators` imported from `layout.typeorm-package` (by name, alias or namespace),
  whose name `image-name-pattern` matches and `image-key-suffix` does not. Reported at its first
  decorator.
- `#cloudfront-infrastructure-only`: a file below `layout.source-root` loads `cloudfront-package` or a subpath.
- `#s3-sdk-placement`: a file below `layout.source-root`, outside `layout.infrastructure-folder`, loads
  `s3-package` or a subpath.

When `central-anchor` exists, central admission exists, and:

- `#s3-adapter-required`: `s3-adapter` is missing (the adapter checks below then stop).
- `#s3-policy-constant`: `s3-policy-constant` is missing, or declares no top-level `UPPER_CASE` const
  (a file without a syntax tree is left to the parse rule); or the adapter declares a top-level
  `UPPER_CASE` const.
- `#s3-sdk-enum`: the adapter sets a property to a string that `forbidden-literals` pairs with its key
  (reported at the property), declares a name `http-status-alias-pattern` matches, or does not use one of
  `required-members`.
- `#central-admission`: a file of an application plane outside `central-internal-folder` names one of
  `admission-names`, holds a string `image-route-pattern` matches, or names something
  `asset-command-pattern` matches; or a file of the domain plane outside `central-domain-folder` names an
  asset command.
- `#domain-storage-access`: a file of the domain plane outside `central-domain-folder` mentions one of
  `storage-access-names` as a whole word in its text.

Findings other than the two noted are reported at the file. Two identical findings are reported once.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `central-admission`,
`cloudfront-infrastructure-only`, `domain-storage-access`, `persisted-image-key`, `s3-adapter-required`,
`s3-policy-constant`, `s3-sdk-enum`, `s3-sdk-placement`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `central-anchor` | path | `layout.modules-root` + `/internal/public-asset/controller.ts` | The file whose presence marks central admission. |
| `central-internal-folder` | path prefix | `layout.modules-root` + `/internal/public-asset/` | The central resource of the internal plane. |
| `central-domain-folder` | path prefix | `layout.modules-root` + `/` + `layout.domain-folder` + `/public-asset/` | The central resource of the domain plane. |
| `s3-adapter` | path | `layout.infrastructure-folder` + `/public-asset/s3/adapter.ts` | The S3 adapter. |
| `s3-policy-constant` | path | `layout.infrastructure-folder` + `/public-asset/constant/s3.constant.ts` | The owner of the S3 policy constants. |
| `s3-package` | string | `"@aws-sdk/client-s3"` | The S3 SDK. |
| `cloudfront-package` | string | `"@aws-sdk/client-cloudfront"` | The CDN SDK. |
| `image-name-pattern` | regular expression | `"(?i)(?:avatar\|banner\|icon\|image\|logo\|photo\|picture\|thumbnail)"` | Names of image fields. |
| `image-key-suffix` | regular expression | `"Key$"` | Names of image references that are keys. |
| `image-route-pattern` | regular expression | `"(?:^\|/)[:A-Za-z0-9_-]*image(?:/\|$)"` | Strings that are image routes. |
| `asset-command-pattern` | regular expression | `"^(?:createWith\|replace\|set\|clear)[A-Za-z0-9_]*(?:Asset\|Image)[A-Za-z0-9_]*$"` | Names of asset commands. |
| `storage-access-names` | list of strings | `["IPublicAssetStorage", "PUBLIC_ASSET_STORAGE_TOKEN_CONSTANT"]` | Names that consume asset storage. |
| `admission-names` | list of strings | `["contentBase64", "declaredMediaType"]` | Names that admit asset bytes. |
| `http-status-alias-pattern` | regular expression | `"^HTTP_STATUS_[A-Z0-9_]+$"` | Local HTTP status aliases. |
| `required-members` | list of `[enum, member]` | `HttpStatus.CONFLICT`, `HttpStatus.NOT_FOUND`, `HttpStatus.PRECONDITION_FAILED`, `ChecksumAlgorithm.SHA256`, `ChecksumMode.ENABLED` | Members the adapter must use. |
| `forbidden-literals` | list of `[key, value]` | `ChecksumAlgorithm: "SHA256"`, `ChecksumMode: "ENABLED"` | Properties the adapter must set through the enum. |

```toml
[rules]
"nestjs/asset-storage" = { level = "error" }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/asset-storage#persisted-image-key src/modules/domain/user/entity/user.entity.ts:4
  src/modules/domain/user/entity/user.entity.ts persists image-like field avatar; persisted image references must end in Key
```

## How to fix it

Admit and store assets only through the central resource and its adapter, persist the asset's key, and
keep the SDKs where the rule places them.

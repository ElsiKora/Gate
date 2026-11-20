# `docker/dockerfile`

Image files are named `Dockerfile`, pinned by digest, built in stages, run as a user that is not root,
carry the OCI labels and no health check, ignore what must never reach the build context, and write
no secret into a build argument or an environment variable.

## What it checks and why

An image is what runs in production, so its file says exactly what goes into it. The rule reads every
tracked file of the project named `Dockerfile` in any case, `Dockerfile.<x>` or `<x>.Dockerfile`, and
every `.dockerignore`. Continuation lines are joined, comments and heredoc bodies skipped, and each
instruction keeps the line where it starts.

| Aspect | Reported when | At |
| --- | --- | --- |
| `name` | an image file is not named `Dockerfile`, or a file is named `.dockerignore` | the file |
| `pinned` | a `FROM` that is not `scratch` or an earlier stage names an image without `@sha256:<64 hex>`, from a registry `registries` does not list, or through a build argument without a default | the `FROM` |
| `stages` | the file has fewer than two stages | the first `FROM` |
| `user` | a runtime stage, or the stage it builds from, ends with no `USER`, or with `root` or `0` | the stage's `FROM` |
| `labels` | a runtime stage does not set each of `labels`, or any `LABEL` sets a name that is not one of them | the stage, or the `LABEL` |
| `healthcheck` | the file declares a `HEALTHCHECK` | the instruction |
| `ignore` | no `<Dockerfile>.dockerignore` is beside the file, or it is no allow-list and does not exclude each of `must-ignore` | the Dockerfile, or the ignore file |
| `secret` | an `ARG` or `ENV`, before the first stage or in one, declares a name that contains one of `secret-names` | the instruction |

A runtime stage is one no later stage builds `FROM` or copies from (`COPY --from`, `RUN
--mount=from=`). A build argument in `FROM` is expanded from the defaults of the `ARG`s before the
first `FROM`, and the default is held to the same pin.

The health check belongs to the orchestrator, which knows when a service is ready for its traffic;
compose files declare it ([`docker/compose`](compose.md)). The ignore file of a Dockerfile is the one
the builder reads beside it, `Dockerfile.dockerignore`; a `.dockerignore` at the context root applies
to every image that shares the context, so the rule asks for the per-file name instead. An ignore file
whose first pattern is `**` is an allow-list and passes; otherwise each pattern of `must-ignore` must
be a line of it, a leading `/` or `**/` and a trailing `/` aside. An `ARG` or `ENV` value is kept in
the image's history, where anyone with the image can read it.

Scope: project. Reads history: no. Aspects: `name`, `pinned`, `stages`, `user`, `labels`,
`healthcheck`, `ignore`, `secret`. Targets: `<path>:<base>` for `pinned`, `<path>:<stage>` for `user`
and a missing label, `<path>:<label>` for a label that is not allowed, `<path>:<line>` for `secret`,
the Dockerfile for a missing ignore file and `<ignore file>:<pattern>` for a missing pattern; a
`name`, `stages` or `healthcheck` finding has none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `must-ignore` | list of patterns | `[".git", ".env", ".env.*", "*.env", "*.pem", "*.key"]` | The patterns an ignore file that is not an allow-list excludes, each as a line. |
| `labels` | list of label names | the three OCI labels `org.opencontainers.image.source`, `.revision` and `.version` | The labels each runtime stage sets, and the only names a `LABEL` may set. |

Both docker rules read `[plugins.docker] options`:

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `secret-names` | list of strings | `["PASSWORD", "PASSWD", "SECRET", "TOKEN", "PRIVATE_KEY", "API_KEY", "ACCESS_KEY", "CREDENTIAL", "AUTH"]` | A variable whose name contains one of these, ignoring case, holds a secret. |
| `registries` | list of registry hosts | `[]` | The registries an image may come from, such as `docker.io`; empty for any. |

```toml
[plugins.docker]
source = "ElsiKora/Gate:docker@0.3.0"
options = { registries = ["docker.io"] }

[rules]
"docker/dockerfile" = "error"
```

## Example finding

`Dockerfile`:

```dockerfile
FROM node:22-alpine
WORKDIR /app
COPY . .
CMD ["node", "main.js"]
```

```text
error docker/dockerfile#pinned Dockerfile:1
  FROM `node:22-alpine` is not pinned by its sha256 digest
  help: pin the base to its digest, `FROM <image>@sha256:<64 hex>`, with the tag in a comment; a build argument's default is pinned the same way
error docker/dockerfile#stages Dockerfile:1
  Dockerfile has one stage
error docker/dockerfile#user Dockerfile:1
  the runtime stage stage 0 runs as root
```

## How to fix it

```dockerfile
# node:22-alpine
ARG NODE=node@sha256:<64 hex>

FROM ${NODE} AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN --mount=type=secret,id=npm npm ci
COPY src ./src
RUN npm run build

FROM ${NODE} AS runtime
ARG SOURCE REVISION VERSION
LABEL org.opencontainers.image.source=$SOURCE \
      org.opencontainers.image.revision=$REVISION \
      org.opencontainers.image.version=$VERSION
WORKDIR /app
COPY --from=build /app/dist ./dist
USER node
CMD ["node", "dist/main.js"]
```

and beside it, `Dockerfile.dockerignore`:

```text
**
!package.json
!package-lock.json
!src
```

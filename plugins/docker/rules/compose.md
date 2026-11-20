# `docker/compose`

Compose files are named for their purpose and their services for their owner, purpose and role;
images are pinned, no secret is written in outside an end-to-end file, ports are published on the
loopback address only, and a service others wait for as healthy declares its health check.

## What it checks and why

A compose file in a repository runs on a developer's machine or in a test job. Two of them in one
directory share a compose project, so their services must not share names; a fixed container name
collides between two worktrees; a port published on every interface opens the service to the
network the machine is on. The rule reads every tracked file of the project named `compose.*` or
`docker-compose.*` with the extension `.yml` or `.yaml`. A file that is not valid YAML stops the run
with the parser's message.

| Aspect | Reported when | At |
| --- | --- | --- |
| `name` | the file is not `docker-compose.<purpose>.yml` or `docker-compose.<scenario>.<purpose>.yml`, with a purpose of `purposes` and a kebab-case scenario | the file |
| `service` | in a well-named file, a service is not `<owner>-<purpose>-<role>` in kebab-case, its second part the file's purpose | the service |
| `container-name` | a service sets `container_name` | the key |
| `pinned` | a service without `build` names an image without `@sha256:<64 hex>` or from a registry `registries` does not list; `${NAME:?...}` passes, `${NAME:-default}` holds its default to the same pin, any other interpolation is reported | the image |
| `secret` | outside an end-to-end file, an `environment` entry whose name contains one of `secret-names` has a literal value | the entry |
| `port` | a `ports` entry is not published on `127.0.0.1` or `::1` | the entry |
| `healthcheck` | a service another names in `depends_on` with `condition: service_healthy` declares no `healthcheck`, or disables it | the service |

**Literal credentials.** An end-to-end file is one whose name ends in `.e2e.yml`. It starts throwaway
services for one test run on the machine running it, so it may write their credentials in. In every
other file, a value is literal when it is written in: not empty and not starting with `$`, or a
`${NAME:-default}` with a non-empty default. A secret-named entry whose name ends in one of
`reference-suffixes` holds a path to the secret, not the secret, and passes. No finding holds a value.

Scope: project. Reads history: no. Aspects: `name`, `service`, `container-name`, `pinned`, `secret`,
`port`, `healthcheck`. Targets: `<path>:<service>` for every aspect but `name` and `secret`, and
`<path>:<line>` for `secret`; a `name` finding has none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `purposes` | list of strings | `["development", "e2e"]` | The purposes a compose file and its services name. |
| `reference-suffixes` | list of strings | `["_FILE", "_PATH"]` | A secret-named variable whose name ends in one of these holds a path, not the secret. |

The rule also reads `secret-names` and `registries` from `[plugins.docker] options`
([`docker/dockerfile`](dockerfile.md#options)).

```toml
[rules]
"docker/compose" = { level = "error", options = { purposes = ["development", "e2e", "demo"] } }
```

## Example finding

`docker-compose.development.yml`:

```yaml
services:
  app-development-database:
    image: postgres@sha256:<64 hex>
    environment:
      POSTGRES_PASSWORD: example
    ports:
      - "5432:5432"
```

```text
error docker/compose#secret docker-compose.development.yml:5
  the service app-development-database has a literal value for POSTGRES_PASSWORD
error docker/compose#port docker-compose.development.yml:7
  the service app-development-database publishes a port beyond the loopback address
  help: publish on 127.0.0.1 or [::1]: `"127.0.0.1::5432"`
```

## How to fix it

```yaml
services:
  app-development-database:
    image: postgres@sha256:<64 hex>  # postgres:17
    environment:
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:?set it in your shell or in .env}
    ports:
      - "127.0.0.1::5432"
    healthcheck:
      test: ["CMD", "pg_isready"]
```

Move throwaway test credentials to `docker-compose.e2e.yml`.

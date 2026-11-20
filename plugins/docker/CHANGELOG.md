# Changelog of the docker plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:docker@<version>`.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Features

- `docker/dockerfile`: an image file is `Dockerfile`, its bases are pinned by digest, it is built in
  stages, its runtime stages run as a user that is not root and carry the OCI labels, it has no
  `HEALTHCHECK`, its `<name>.dockerignore` keeps secrets out of the context, and no build argument or
  environment variable holds a secret.
- `docker/compose`: a compose file is `docker-compose.[<scenario>.]<purpose>.yml`, its services are
  `<owner>-<purpose>-<role>` without `container_name`, its images are pinned, it writes no secret
  outside an end-to-end file, it publishes ports on the loopback address only, and a service others
  wait for as healthy declares its health check.
- Presets `docker/recommended` (both rules at `error`) and `docker/adoption` (both at `warning`).

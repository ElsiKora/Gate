# `repository/large-files`

No large file is tracked unless the project declares it with its job.

## What it checks and why

Every clone downloads every version of every tracked file. A large file belongs in Git LFS or in
storage the build fetches from; one the repository needs, such as a logo the site serves, is declared
with what it is for, so the next reader knows why it stays. The rule asks the size of each tracked file
of the repository and reads no content, and its result stays cached while the sizes do.

The size is that of the blob git stores. In a commit or the index it is the blob's size. In the
working tree, a file that matches its index entry (unchanged since git wrote or staged it) has the
size of the index's blob, and only a file that differs from its index entry has its own size. So a
file Git LFS moved has its pointer's size in every mode, as the pre-commit hook sees it, though the
checkout holds the whole file.

| Aspect | Reported when | At |
| --- | --- | --- |
| `large` | a file is over `large-bytes` and not over `huge-bytes` | the file |
| `huge` | a file is over `huge-bytes` | the file |
| `extra-missing` | an entry of `extra` names a path that is not tracked | none |

A file `extra` names is not reported. `huge-bytes` below `large-bytes` is a configuration error.

Scope: repository. Reads history: no. Aspects: `large`, `huge`, `extra-missing`. Targets: the path.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `large-bytes` | integer | `1048576` (1 MiB) | A file over this many bytes is `large`. |
| `huge-bytes` | integer | `5242880` (5 MiB) | A file over this many bytes is `huge`, and not also `large`. |
| `extra` | list of `{ path, job }` | `[]` | The large files the repository needs, each with what it is for. |

`repository/recommended` sets `large` to `warning` and `huge` to `error`.

```toml
[rules."repository/large-files"]
level = "error"
options = { extra = [{ path = "site/public/hero.png", job = "the image the landing page serves" }] }
```

## Example finding

```text
error repository/large-files#huge assets/demo.mp4
  assets/demo.mp4 is 7340032 bytes, over 5242880
  help: move it to Git LFS or to storage, or, when the project needs it here, declare it in `extra` with its job
```

## How to fix it

Move the file to Git LFS or to storage the build fetches from, and remove it from the tree. When the
repository needs it, add `{ path, job }` to `extra`. A file removed from the tree stays in the history;
rewriting the history is a decision of its own.

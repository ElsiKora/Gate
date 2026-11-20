# T-066b: Secret identity, key bodies and filtered sizes

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | 0.3.0, merged right after T-066 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `task/T-066-containers-hygiene` at `9cd77bb` (T-066 accepted) |
| Touches | `plugins/repository/src/tracked_secrets.rs` and its page and tests, `crates/gate-host/src/snapshot.rs`, `crates/gate-host/src/git.rs`, the host's snapshot tests |

## Goal

The three minor findings of the lead's independent re-check of T-066, one with security weight:

1. **A secret's identity is tied to that secret.** In T-066 a content finding of
   `repository/tracked-secrets` is identified by `<path>:<kind>[#n]`, an ordinal in file order. A real
   key added above an excepted fixture then takes the fixture's identity and is excepted without a
   word. The identity becomes `<kind>:<the first 12 hex of the sha256 of the matched text>`: the key
   is that, and the target `<path>:<key>`. It is stable across edits anywhere else and names one
   secret only. The hash is of a secret, but 48 bits of a sha256 do not reveal a key or a token
   whose shape holds at least 128 bits.
2. **A key body is a contiguous base64 run.** Between a private-key header and `-----END `, at least
   64 base64 characters must follow one another, whitespace and `\n` or `\r` escapes aside. PEM
   parsing code that names the header and footer as constants, or prose, then gives no finding,
   and a real one-line key still does. The search for `-----END ` is bounded so that many headers
   before one footer stay linear.
3. **A filtered working file has the size of the blob git would store.** `git diff-files` lists a
   file whose stat changed but whose content did not (`cp -r`, a CI cache restore, `touch`). For a
   listed path git converts (a filter, line endings), `tree.size` answers the size of the blob
   `git hash-object` gives with the clean filter run, so a touched or modified Git LFS file has its
   pointer's size. A blob not yet in the object database is written to a temporary object
   directory, never to the repository's.

## Done when

- Tests: an excepted fixture with a new key added above it, and the new key is reported; header and
  footer constants in code give no finding while a one-line key does; many headers before one footer;
  a touched and a modified filtered file in working-tree mode have the stored blob's size.
- One lane round on the pushed tip, on T-066b's own target directories: fmt, clippy, test
  (`--workspace --no-fail-fast`), deny and the release build exit 0, every changed crate compiled from
  T-066b's checkout and every new test listed; the leak scan prints nothing.

## Resume point

2026-11-20T10:21:37+00:00: branch `task/T-066b-hygiene-identity` from `9cd77bb`; task file written.
Next: the three fixes and their tests, then the lane round.

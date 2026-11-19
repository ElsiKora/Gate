# House style of ElsiKora public repositories

Our own reference for presenting a public repository the way the company's other public repositories
are presented. Read on 2026-11-16 with `gh repo view`, `gh api repos/ElsiKora/<repo>/readme`,
`gh api repos/ElsiKora/<repo>/contents/<path>`, `gh api repos/ElsiKora/<repo>/community/profile` and
`gh release view -R ElsiKora/<repo>`.

## Repositories read

| Repository | Kind | What was taken from it |
| --- | --- | --- |
| [NestJS-Crud-Automator](https://github.com/ElsiKora/NestJS-Crud-Automator) | TypeScript library | README section order, changelog format, `docs/` as a separate guide tree, `## 🧭 Migrating to N` sections |
| [ESLint-Config](https://github.com/ElsiKora/ESLint-Config) | TypeScript configuration | header block (logo, emoji title, italic tagline, badge row), `✨` feature bullets, roadmap table, release configuration, dependabot, branch names |
| [NestJS-Crud-Config](https://github.com/ElsiKora/NestJS-Crud-Config) | TypeScript library | `### Prerequisites` under installation, reference sections after usage |
| [Git-Branch-Lint](https://github.com/ElsiKora/Git-Branch-Lint) | TypeScript CLI | a linter's README: configuration examples per format, short development section |
| [Claude-Switcher](https://github.com/ElsiKora/Claude-Switcher) | Swift application | the newest README: `💡 Highlights`, `🛠️ Tech Stack`, `🏗 Architecture` with mermaid, `📁 Project Structure`, `🔒 Privacy and Security`, FAQ in `<details>`, `🙏 Acknowledgments`, `Back to Top`; release notes with SHA-256 lines; a non-npm release |
| [ClaDI](https://github.com/ElsiKora/ClaDI) | TypeScript library | `<a id="top">` anchor, `💡 Highlights` before the table of contents, `⚠️ Common Pitfalls` |

## README

Exact order of the header, as in ESLint-Config, ClaDI and Claude-Switcher:

```html
<a id="top"></a>

<p align="center">
  <img src="<logo url>" width="700" alt="project-logo">
</p>

<h1 align="center"><emoji> <Name></h1>
<p align="center"><em><one sentence: what it is and for whom></em></p>

<p align="center">
    <a aria-label="ElsiKora logo" href="https://elsikora.com">
  <img src="https://img.shields.io/badge/MADE%20BY%20ElsiKora-333333.svg?style=for-the-badge" alt="ElsiKora">
</a> <img src="https://img.shields.io/badge/<Tech>-<hex>.svg?style=for-the-badge&logo=<slug>&logoColor=white" alt="<Tech>"> ...
</p>
```

- Badges are static shields.io badges in `for-the-badge` style, one per technology, the first always
  `MADE BY ElsiKora` linking to `https://elsikora.com`. Claude-Switcher adds a licence badge at the end.
- The logo is an image 500 to 700 px wide; Claude-Switcher keeps it in `docs/assets/logo.png`, older
  repositories on a public blob store. Claude-Switcher's logo (drawn by its `scripts/render-logo.swift`)
  sets the current pattern: a dark rounded-square icon (`#1F1F1F`) with a mark in a purple-to-magenta
  gradient (`#7312D4` to `#D11CC7`), the name in heavy capitals in the same gradient (magenta at the top),
  and a two-colour rule along the bottom of a 1920 x 1080 canvas. Gate's `docs/assets/logo.svg` follows
  it as a vector drawing with no font, on a transparent background for light and dark themes.
- A `> [!NOTE]` callout may follow the badges (Claude-Switcher: the unofficial-tool note).

Sections, each an `##` heading with an emoji, in this order (Claude-Switcher, the most complete; the
older repositories use a subset in the same order):

| Heading | Content |
| --- | --- |
| `## 💡 Highlights` | three or four bullets, each starting with its own emoji (ClaDI, Claude-Switcher) |
| `## 📚 Table of Contents` | a bullet list of links to the sections below, anchors as GitHub makes them (`#-description`) |
| `## 📖 Description` | two or three paragraphs; then `### Real-World Use Cases` with bold-led bullets |
| `## 🛠️ Tech Stack` | a two-column table, `Category` and `Technologies`, categories in bold |
| `## 🚀 Features` | bullets of the form `- ✨ **Name**: what it does.` |
| `## 🏗 Architecture` | `### System Architecture` with a mermaid `flowchart`, then a sequence or data flow |
| `## 📁 Project Structure` | a tree in a plain fenced block with `# comment` per entry |
| `## 📋 Prerequisites` | a short list |
| `## 🛠 Installation` | `### From a Release`, `### From Source`, `### Development Setup` |
| `## 💡 Usage` | `###` subsections, one per task, with commands; callouts `> [!IMPORTANT]`, `> [!TIP]`, `> [!WARNING]` |
| topic sections | for example `## 🔒 Privacy and Security`, `## 🧭 Migrating to 4.0`, `## ⚠️ Common Pitfalls` |
| `## 🛣 Roadmap` | a table `Task / Feature` and `Status`, statuses `✅ Done` and `🚧 In Progress` |
| `## ❓ FAQ` | each question in `<details><summary><strong>...</strong></summary>` (Claude-Switcher, ClaDI) |
| `## 🔒 License` | one sentence naming the licence, a link to `LICENSE` |
| `## 🙏 Acknowledgments` | built and maintained by ElsiKora; the projects it builds on |

The README ends with a rule and `<p align="center"><a href="#top">Back to Top</a></p>`.

Tone: second person, plain sentences, concrete numbers; the newest README (Claude-Switcher) is the model,
not the generated feature lists of the older ones.

## Licence

- MIT in every TypeScript repository read (`LICENSE`, `Copyright (c) 2025 ElsiKora`, the standard text);
  Apache-2.0 with a `NOTICE` in Claude-Switcher, which derives from an Apache-2.0 project.
- Gate takes MIT, holder `ElsiKora`, year of the first public release; the workspace's `Cargo.toml`
  already says `license = "MIT"`.

## Changelog and releases

- `CHANGELOG.md` is written by semantic-release (`@semantic-release/changelog`, preset
  `conventionalcommits`) in every TypeScript repository. Exact shape:

  ```markdown
  # [1.2.0](https://github.com/ElsiKora/<Repo>/compare/v1.1.2...v1.2.0) (2026-02-14)

  ### Bug Fixes

  - **scope:** subject ([abc1234](https://github.com/ElsiKora/<Repo>/commit/<full sha>))

  ### Features

  - **scope:** subject ([abc1234](https://github.com/ElsiKora/<Repo>/commit/<full sha>))
  ```

  A minor or major release is an `#` heading, a patch an `##` heading; newest first; no preamble.
- Commits are Conventional Commits (`commitlint.config.js` extends `@commitlint/config-conventional`);
  `feat` makes a minor release, every other type a patch, a breaking change a major
  (`release.config.js` of ESLint-Config).
- Tags are `v<semver>`; the GitHub release is named after the tag; its body is the changelog entry.
  Prereleases come from a `dev` branch as `v<next>-dev.<n>` (ESLint-Config, NestJS-Crud-Automator).
- A release that ships binaries lists `SHA-256:` and one line per asset in its body (Claude-Switcher).
- Release commits: `chore(release): <version> [skip ci]`.

Gate follows the same changelog shape and commit rules. Its release job is part of task T-004; it has to
write the same headings and the SHA-256 lines.

## Branches and automation

- Branch names `:type/:name` with types `feature`, `bugfix`, `hotfix`, `release`, `support`, lowercase
  kebab subject, 5 to 50 characters, never `main` (`.elsikora/git-branch-lint.config.js` of
  ESLint-Config).
- `.github/dependabot.yml`: the package ecosystem and `github-actions`, daily. Workflows: `test.yml`
  (on push), `release.yml`, a security scan and a quality scan.

## Community files

`gh api repos/ElsiKora/<repo>/community/profile` reports no `CONTRIBUTING`, no `CODE_OF_CONDUCT`, no
issue template and no pull request template in any of the six repositories; none has a `SECURITY.md`.
Gate is the first public repository to carry them, so their pattern starts here:

- `CONTRIBUTING.md`, `SECURITY.md`: the README's tone, emoji `##` headings, no table of contents.
- `.github/ISSUE_TEMPLATE/*.yml`: GitHub issue forms, bug report and feature request, blank issues off.
- `.github/PULL_REQUEST_TEMPLATE.md`: what changed, why, how it was checked.
- No code of conduct, as in the house.

## Guides

NestJS-Crud-Automator keeps a guide tree in `docs/` (`getting-started`, `core-concepts`, `guides`,
`api-reference`), mirrored to a documentation site. Gate keeps its guides as plain Markdown in
`docs/guide/`, linked from the README, so they read on GitHub without a site.

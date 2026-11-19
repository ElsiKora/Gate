//! The standards, side by side: the top-level entries of `docs/` and the required sections of each
//! entry's index, for every [`Kind`]. A cell that is not the same in every kind carries its reason;
//! the plugin's tests fail a difference without one.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;

/// A kind of project. A project names its kind in the `[plugins.docs]` options; it is never
/// inferred.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Kind {
    /// A service: an API with its data (the `NestJS` services, whatever the framework).
    Service,
    /// A frontend: a single-page or server-rendered web application.
    Frontend,
    /// Infrastructure: environments, networks and their secrets, as code.
    Infrastructure,
    /// A library: a package published for others to use (a component library, an SDK).
    Library,
    /// The root of a monorepo: its member projects keep their own `docs/`.
    MonorepoRoot,
}

impl Kind {
    /// Every kind, in the order of the tables' columns.
    pub const ALL: [Self; 5] = [
        Self::Service,
        Self::Frontend,
        Self::Infrastructure,
        Self::Library,
        Self::MonorepoRoot,
    ];

    /// The column of the kind in the tables.
    #[must_use]
    pub const fn column(self) -> usize {
        match self {
            Self::Service => 0,
            Self::Frontend => 1,
            Self::Infrastructure => 2,
            Self::Library => 3,
            Self::MonorepoRoot => 4,
        }
    }

    /// The kind as the options name it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Frontend => "frontend",
            Self::Infrastructure => "infrastructure",
            Self::Library => "library",
            Self::MonorepoRoot => "monorepo-root",
        }
    }

    /// The kind in a sentence: "a service".
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Service => "a service",
            Self::Frontend => "a frontend",
            Self::Infrastructure => "an infrastructure project",
            Self::Library => "a library",
            Self::MonorepoRoot => "a monorepo root",
        }
    }
}

/// Every kind's name, for messages: `service, frontend, …`.
#[must_use]
pub fn kind_names() -> String {
    Kind::ALL.map(Kind::name).join(", ")
}

/// An entry's cell in one kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cell {
    /// The entry exists with its job, as in the other kinds.
    Same,
    /// The kind has no such entry, for the reason given (a clause, completing "… is not an entry of
    /// a monorepo root: ").
    Absent(&'static str),
}

/// A top-level entry of `docs/`.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// The name under `docs/`: a directory (`architecture`) or a Markdown file (`start-here.md`).
    pub name: &'static str,
    /// Whether the entry is a directory, whose index is its `README.md`.
    pub directory: bool,
    /// What the entry holds.
    pub job: &'static str,
    /// Present only when the project has that job, the same in every kind.
    pub optional: bool,
    /// The plugin that owns what is inside: the layout never requires, seeds or reads it.
    pub delegated: Option<&'static str>,
    /// The entry in each kind, in the order of [`Kind::ALL`].
    pub cells: [Cell; 5],
}

impl Entry {
    /// The entry's cell in `kind`.
    #[must_use]
    pub const fn cell(&self, kind: Kind) -> Cell {
        self.cells[kind.column()]
    }

    /// Whether `kind` has the entry.
    #[must_use]
    pub fn in_kind(&self, kind: Kind) -> bool {
        self.cell(kind) == Cell::Same
    }

    /// Whether `kind` requires it: it has the entry, and it is neither optional nor delegated.
    #[must_use]
    pub fn required(&self, kind: Kind) -> bool {
        self.in_kind(kind) && !self.optional && self.delegated.is_none()
    }

    /// The entry's path in the project: `docs/architecture/` or `docs/start-here.md`.
    #[must_use]
    pub fn path(&self) -> String {
        if self.directory {
            format!("docs/{}/", self.name)
        } else {
            format!("docs/{}", self.name)
        }
    }

    /// The path of the entry's index: `docs/architecture/README.md` or `docs/start-here.md`.
    #[must_use]
    pub fn index(&self) -> String {
        if self.directory {
            format!("docs/{}/README.md", self.name)
        } else {
            format!("docs/{}", self.name)
        }
    }
}

const S: Cell = Cell::Same;
const ALL_SAME: [Cell; 5] = [S; 5];

/// Why a monorepo root has no `modules/`.
pub const ROOT_NO_MODULES: &str = "the root owns no code; its parts are member projects, each with its own docs/, and the root's index links them under Members";
/// Why a monorepo root has no `ledger/`.
pub const ROOT_NO_LEDGER: &str = "work is recorded in the ledger of the member project whose code it changes, and a repository-wide decision is an adr/ record";

/// The top-level entries, in the standard order: the order of the index's entry table.
pub const ENTRIES: &[Entry] = &[
    Entry {
        name: "README.md",
        directory: false,
        job: "the index: the reading order and a table of every entry",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "start-here.md",
        directory: false,
        job: "the first reading path, for a person or an agent",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "architecture",
        directory: true,
        job: "how the system is built and why",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "modules",
        directory: true,
        job: "a chapter per owned part of the code: a module, a feature, a stack, a package",
        optional: false,
        delegated: None,
        cells: [S, S, S, S, Cell::Absent(ROOT_NO_MODULES)],
    },
    Entry {
        name: "adr",
        directory: true,
        job: "decision records: the log and one NNNN-<slug>.md per decision",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "reference",
        directory: true,
        job: "exact values and contracts: formats, inventories, settings, the glossary",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "guide",
        directory: true,
        job: "task-oriented chapters: how to do one thing, with examples",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "testing",
        directory: true,
        job: "what the tests are, where they live, what counts as evidence",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "operations",
        directory: true,
        job: "runbooks: deploy, roll back, recover, rotate, release",
        optional: false,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "agents",
        directory: true,
        job: "agent rules, skills and the agents' reading map",
        optional: false,
        delegated: Some("agents"),
        cells: ALL_SAME,
    },
    Entry {
        name: "ledger",
        directory: true,
        job: "work records, verification, follow-ups",
        optional: false,
        delegated: Some("ledger"),
        cells: [S, S, S, S, Cell::Absent(ROOT_NO_LEDGER)],
    },
    Entry {
        name: "research",
        directory: true,
        job: "exploration that informed a decision but is not one",
        optional: true,
        delegated: None,
        cells: ALL_SAME,
    },
    Entry {
        name: "releases",
        directory: true,
        job: "one page per released version: what a consumer changes",
        optional: true,
        delegated: None,
        cells: ALL_SAME,
    },
];

/// A name refused even as a declared extension, because its job has an entry.
#[derive(Clone, Copy, Debug)]
pub struct Reserved {
    /// The name under `docs/`.
    pub name: &'static str,
    /// Where its content goes.
    pub goes_to: &'static str,
}

/// The reserved names. A name without `.md` is reserved as a directory and as a Markdown file.
pub const RESERVED: &[Reserved] = &[
    Reserved {
        name: "concepts",
        goes_to: "docs/architecture/",
    },
    Reserved {
        name: "explanation",
        goes_to: "docs/architecture/",
    },
    Reserved {
        name: "how-to",
        goes_to: "docs/guide/",
    },
    Reserved {
        name: "howto",
        goes_to: "docs/guide/",
    },
    Reserved {
        name: "tutorials",
        goes_to: "docs/guide/",
    },
    Reserved {
        name: "guides",
        goes_to: "docs/guide/",
    },
    Reserved {
        name: "components",
        goes_to: "docs/modules/",
    },
    Reserved {
        name: "features",
        goes_to: "docs/modules/",
    },
    Reserved {
        name: "stacks",
        goes_to: "docs/modules/",
    },
    Reserved {
        name: "packages",
        goes_to: "docs/modules/",
    },
    Reserved {
        name: "ai",
        goes_to: "docs/agents/",
    },
    Reserved {
        name: "decisions",
        goes_to: "docs/adr/",
    },
    Reserved {
        name: "runbooks",
        goes_to: "docs/operations/",
    },
    Reserved {
        name: "glossary.md",
        goes_to: "docs/reference/glossary.md",
    },
];

/// The reserved name `name` matches: itself, or, for a name without `.md`, the Markdown file of
/// that name.
#[must_use]
pub fn reserved(name: &str) -> Option<&'static Reserved> {
    RESERVED.iter().find(|reserved| {
        reserved.name == name
            || (reserved.name.strip_suffix(".md").is_none()
                && name.strip_suffix(".md") == Some(reserved.name))
    })
}

/// The standard entry named `name`, whatever its form.
#[must_use]
pub fn entry(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.name == name)
}

/// A required level-2 section of an index, and what goes there (the line the seed writes under it).
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub title: &'static str,
    pub says: &'static str,
}

const fn section(title: &'static str, says: &'static str) -> Section {
    Section { title, says }
}

/// The required sections of one entry's index, per kind.
#[derive(Clone, Copy, Debug)]
pub struct Index {
    /// The entry, as [`Entry::name`].
    pub entry: &'static str,
    /// The page's title, its level-1 heading.
    pub title: &'static str,
    /// The sections in each kind, in the order of [`Kind::ALL`]: required, in this order.
    pub sections: [&'static [Section]; 5],
    /// Why a kind's sections differ from the most kinds', or, when no two kinds agree, from each
    /// other's; `None` where they do not differ.
    pub reasons: [Option<&'static str>; 5],
}

impl Index {
    /// The section titles in `kind`.
    #[must_use]
    pub fn titles(&self, kind: Kind) -> Vec<&'static str> {
        self.sections[kind.column()]
            .iter()
            .map(|section| section.title)
            .collect()
    }
}

const SYSTEM_SHAPE: Section = section(
    "System shape",
    "The parts of the system and how they connect.",
);

const README: &[Section] = &[
    section(
        "Reading order",
        "The order in which to read these pages, for a person and for an agent.",
    ),
    section(
        "Entries",
        "One row per entry of this book, in the standard order, each a link, with its job.",
    ),
];
const ROOT_README: &[Section] = &[
    README[0],
    README[1],
    section(
        "Members",
        "One row per member project, each a link to its docs/README.md.",
    ),
];
const START_HERE: &[Section] = &[
    section(
        "Mental model",
        "What this project is, in a few paragraphs, and the words it uses.",
    ),
    section("First day", "What to read, run and change first."),
];
const SERVICE_ARCHITECTURE: &[Section] = &[
    SYSTEM_SHAPE,
    section(
        "API surface",
        "The interfaces the service offers and the ones it calls.",
    ),
    section("Data", "What the service stores, where, and who owns it."),
    section(
        "Runtime",
        "How the service runs: processes, jobs, scaling, configuration.",
    ),
];
const FRONTEND_ARCHITECTURE: &[Section] = &[
    SYSTEM_SHAPE,
    section("Routing", "The routes, and how a request reaches a page."),
    section(
        "State",
        "Where state lives: server data, client state, the URL.",
    ),
    section(
        "Rendering",
        "Where pages render (in the browser, on the server, or both), and why.",
    ),
    section(
        "API access",
        "How the frontend calls its back ends, and how it handles their failures.",
    ),
];
const INFRASTRUCTURE_ARCHITECTURE: &[Section] = &[
    SYSTEM_SHAPE,
    section(
        "Environments",
        "The environments, and what differs between them.",
    ),
    section(
        "Networking",
        "The networks, their boundaries, and what may reach what.",
    ),
    section(
        "Secrets and parameters",
        "Where secrets and parameters live, and how they reach a workload.",
    ),
];
const LIBRARY_ARCHITECTURE: &[Section] = &[
    SYSTEM_SHAPE,
    section(
        "Public API",
        "What the library exports for others to use; everything else is internal.",
    ),
    section(
        "Versioning and release",
        "How versions are numbered and released, and what counts as a breaking change.",
    ),
    section(
        "Compatibility",
        "The runtimes, platforms and peer versions the library supports.",
    ),
];
const ROOT_ARCHITECTURE: &[Section] = &[
    SYSTEM_SHAPE,
    section(
        "Members and dependencies",
        "The member projects, and which depends on which.",
    ),
    section(
        "Build and release flow",
        "The order in which the members build, test and release.",
    ),
];
const MODULES: &[Section] = &[
    section(
        "Chapter map",
        "One row per chapter: the part of the code, its owner, its page.",
    ),
    section(
        "Adding a module",
        "How to add a chapter when a part of the code is added.",
    ),
];
const ADR: &[Section] = &[
    section(
        "Log",
        "One row per decision record, oldest first, with its status.",
    ),
    section(
        "Template",
        "The template of a record; copy it to NNNN-<slug>.md.",
    ),
];
const REFERENCE: &[Section] = &[section(
    "Contents",
    "One row per reference page: formats, inventories, settings, the glossary.",
)];
const GUIDE: &[Section] = &[section("Pages", "One row per guide page, each one task.")];
const TESTING: &[Section] = &[
    section(
        "Kinds of test",
        "The kinds of test, where each lives, and how to run it.",
    ),
    section("Evidence", "What counts as evidence that a change works."),
];
const OPERATIONS: &[Section] = &[section(
    "Runbooks",
    "One row per runbook: deploy, roll back, recover, rotate, release.",
)];

const NO_REASONS: [Option<&str>; 5] = [None; 5];

/// The required sections of every index, in the standard order of their entries.
pub const INDEXES: &[Index] = &[
    Index {
        entry: "README.md",
        title: "Documentation",
        sections: [README, README, README, README, ROOT_README],
        reasons: [
            None,
            None,
            None,
            None,
            Some(
                "a root's parts are member projects, and a reader needs a table that links each member's docs/README.md",
            ),
        ],
    },
    Index {
        entry: "start-here.md",
        title: "Start here",
        sections: [START_HERE; 5],
        reasons: NO_REASONS,
    },
    Index {
        entry: "architecture",
        title: "Architecture",
        sections: [
            SERVICE_ARCHITECTURE,
            FRONTEND_ARCHITECTURE,
            INFRASTRUCTURE_ARCHITECTURE,
            LIBRARY_ARCHITECTURE,
            ROOT_ARCHITECTURE,
        ],
        reasons: [
            Some("a service has an API and the data behind it"),
            Some("a frontend has routes, state and a rendering model"),
            Some("infrastructure has environments, a network and the secrets that cross it"),
            Some(
                "a library has a public API that others build on, a release line and the versions it supports",
            ),
            Some("a root has members and the order in which they build"),
        ],
    },
    Index {
        entry: "modules",
        title: "Modules",
        sections: [MODULES; 5],
        reasons: NO_REASONS,
    },
    Index {
        entry: "adr",
        title: "Decision records",
        sections: [ADR; 5],
        reasons: NO_REASONS,
    },
    Index {
        entry: "reference",
        title: "Reference",
        sections: [REFERENCE; 5],
        reasons: NO_REASONS,
    },
    Index {
        entry: "guide",
        title: "Guide",
        sections: [GUIDE; 5],
        reasons: NO_REASONS,
    },
    Index {
        entry: "testing",
        title: "Testing",
        sections: [TESTING; 5],
        reasons: NO_REASONS,
    },
    Index {
        entry: "operations",
        title: "Operations",
        sections: [OPERATIONS; 5],
        reasons: NO_REASONS,
    },
];

/// The indexes `kind` requires, each with its entry, in the standard order.
pub fn required_indexes(kind: Kind) -> impl Iterator<Item = (&'static Entry, &'static Index)> {
    INDEXES.iter().filter_map(move |index| {
        let entry = entry(index.entry)?;
        entry.required(kind).then_some((entry, index))
    })
}

/// The line a seed carries until a person fills the page; the rule reports it as `unfilled`.
pub const SEED_MARKER: &str = "<!-- gate:seed: replace this page's text, then delete this line -->";

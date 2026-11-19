//! The vocabulary of the field classification: API planes, classes, code kinds, the fields read
//! from the source and the entries of the `fields` option.

use std::collections::BTreeSet;
use std::fmt;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;

/// An API plane a field can be exposed on.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd)]
#[serde(crate = "gate_sdk::serde", rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) enum Plane {
    Integration,
    Internal,
    Public,
}

impl Plane {
    pub(crate) const ALL: [Self; 3] = [Self::Integration, Self::Internal, Self::Public];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Integration => "INTEGRATION",
            Self::Internal => "INTERNAL",
            Self::Public => "PUBLIC",
        }
    }
}

/// The class a field declares: one of the six text classes, a code or a machine
/// value that is no text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd)]
#[serde(crate = "gate_sdk::serde", rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) enum Class {
    ProperName,
    AuthoredContent,
    PlayerContent,
    PersonalData,
    StaffNote,
    ThirdParty,
    Code,
    Machine,
}

impl Class {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ProperName => "PROPER_NAME",
            Self::AuthoredContent => "AUTHORED_CONTENT",
            Self::PlayerContent => "PLAYER_CONTENT",
            Self::PersonalData => "PERSONAL_DATA",
            Self::StaffNote => "STAFF_NOTE",
            Self::ThirdParty => "THIRD_PARTY",
            Self::Code => "CODE",
            Self::Machine => "MACHINE",
        }
    }

    /// Whether the class may appear only on the internal plane unless a decision exposes it.
    pub(crate) fn is_private(self) -> bool {
        matches!(self, Self::StaffNote | Self::ThirdParty)
    }
}

/// The format kind of a code.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd)]
#[serde(crate = "gate_sdk::serde", rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) enum CodeKind {
    UpperSnake,
    Kebab,
    #[serde(rename = "ISO_4217")]
    Iso4217,
    MarketTicker,
    #[serde(rename = "ISO_3166_ALPHA2")]
    Iso3166Alpha2,
    #[serde(rename = "ISO_639_1")]
    Iso6391,
    ConfigKey,
    /// An internal stable key: it names a record among its siblings, is never
    /// displayed and is referenced by no configuration; as an `IDENTITY` code it is immutable and
    /// unique within its owner scope, and a column that names one is a `REFERENCE` code of this
    /// kind.
    StableKey,
}

impl CodeKind {
    /// The grammar every code of this kind matches, as the body of an Automator `pattern`.
    pub(crate) fn grammar(self) -> &'static str {
        match self {
            Self::UpperSnake => "^[A-Z][A-Z0-9]*(_[A-Z0-9]+)*$",
            Self::Kebab | Self::ConfigKey | Self::StableKey => "^[a-z0-9]+(-[a-z0-9]+)*$",
            Self::Iso4217 | Self::MarketTicker => "^[A-Z]{3,5}$",
            Self::Iso3166Alpha2 => "^[A-Z]{2}$",
            Self::Iso6391 => "^[a-z]{2}$",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::UpperSnake => "UPPER_SNAKE",
            Self::Kebab => "KEBAB",
            Self::Iso4217 => "ISO_4217",
            Self::MarketTicker => "MARKET_TICKER",
            Self::Iso3166Alpha2 => "ISO_3166_ALPHA2",
            Self::Iso6391 => "ISO_639_1",
            Self::ConfigKey => "CONFIG_KEY",
            Self::StableKey => "STABLE_KEY",
        }
    }
}

/// What a persisted code is to its record.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd)]
#[serde(crate = "gate_sdk::serde", rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) enum Role {
    /// The code identifies its own record, so it is immutable and unique within its owner scope.
    Identity,
    /// The code names another record's code or a vocabulary, such as a snapshot or a reason.
    Reference,
}

impl Role {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Identity => "IDENTITY",
            Self::Reference => "REFERENCE",
        }
    }
}

/// The members a heritage edge keeps of the class it builds on, composed along the nested mapped
/// types (`PickType`, `OmitType`, `PartialType`, `IntersectionType`) between the class and that
/// base: within every picked key list and outside every omitted one. A key list that cannot be
/// read is not composed, so the edge keeps more members than it may (fail closed).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Kept {
    /// The keys every `PickType` on the edge lists in common; `None` when no `PickType` is read.
    pick: Option<BTreeSet<String>>,
    /// The keys some `OmitType` on the edge lists.
    omit: BTreeSet<String>,
}

impl Kept {
    /// The edge also passes a `PickType` of `keys`.
    pub(crate) fn picked(&self, keys: &BTreeSet<String>) -> Self {
        Self {
            pick: Some(match &self.pick {
                Some(pick) => pick.intersection(keys).cloned().collect(),
                None => keys.clone(),
            }),
            omit: self.omit.clone(),
        }
    }

    /// The edge also passes an `OmitType` of `keys`.
    pub(crate) fn omitted(&self, keys: &BTreeSet<String>) -> Self {
        Self {
            pick: self.pick.clone(),
            omit: self.omit.union(keys).cloned().collect(),
        }
    }

    /// Whether the edge keeps `member`.
    pub(crate) fn keeps(&self, member: &str) -> bool {
        self.pick.as_ref().is_none_or(|pick| pick.contains(member)) && !self.omit.contains(member)
    }
}

/// The Automator `pattern` of a field.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Pattern {
    /// The field declares none.
    #[default]
    Absent,
    /// Its options or its value cannot be read.
    Unread,
    /// Its text, a `/…/` regular expression as Automator takes it.
    Text(String),
}

/// A class member by its declaring file and class.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Key {
    pub path: String,
    pub class_name: String,
    pub member: String,
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.class_name, self.member)
    }
}

/// Where a field is declared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Origin {
    Entity,
    Dto,
}

/// Whether a field can hold free text, as its persisted type, its TypeScript type and its
/// Automator metadata show.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Scope {
    /// It can hold free text and needs an entry.
    Text,
    /// An Automator code format that names its kind; no entry is needed.
    Code(CodeKind, String),
    /// It cannot hold free text, for the reason given.
    Excluded(String),
}

/// One route and DTO type of a generated Automator controller.
pub(crate) type Slot = (&'static str, &'static str);

/// What a generated DTO of an entity field reads from its `ApiPropertyDescribe`.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct AutoDto {
    /// `isAutoDtoEnabled: false` keeps it out of every generated DTO.
    pub disabled: bool,
    /// The route and DTO type pairs whose `properties` set `isEnabled: false`.
    pub excluded: BTreeSet<Slot>,
    /// Its Automator type is `OBJECT`, which no generated query filters by.
    pub object: bool,
    /// It carries `ApiPropertyDescribe`; Automator generates no DTO or filter property without it.
    pub described: bool,
}

/// One entity column or DTO property read from the source.
#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub key: Key,
    pub line: u32,
    pub origin: Origin,
    pub scope: Scope,
    pub auto: AutoDto,
    pub pattern: Pattern,
}

/// One classified field of the options.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) struct Entry {
    /// The file that declares the field.
    pub path: String,
    /// The entity or DTO class that declares the field.
    pub class_name: String,
    /// The property name of the field.
    pub member: String,
    /// What the field holds.
    pub class: Class,
    /// The grammar of a `CODE` field; none for another class.
    #[serde(default)]
    pub kind: Option<CodeKind>,
    /// The accepted decision that owns the classification.
    pub decision: String,
    /// The planes the field reaches.
    pub exposure: Vec<Plane>,
    /// The accepted decision that exposes a `STAFF_NOTE` or `THIRD_PARTY` field beyond `INTERNAL`.
    #[serde(default)]
    pub exposing_decision: Option<String>,
    /// Why a `MACHINE` value holds no text; none for another class.
    #[serde(default)]
    pub reason: Option<String>,
    /// Another class the owner may still choose instead; the report lists it as an open choice.
    #[serde(default)]
    pub alternative: Option<Class>,
    /// The configuration and secret contracts that reference a `CONFIG_KEY` code.
    #[serde(default)]
    pub reference: Vec<Reference>,
    /// What a persisted code is to its record.
    #[serde(default)]
    pub role: Option<Role>,
    /// The other columns of an `IDENTITY` code's unique constraint, as the `TypeORM` decorator lists
    /// them; empty for a code unique across its table.
    #[serde(default)]
    pub scope: Option<Vec<String>>,
}

impl Entry {
    /// Whether the entry is a code of the configuration-key kind.
    pub(crate) fn is_config_key(&self) -> bool {
        self.class == Class::Code && self.kind == Some(CodeKind::ConfigKey)
    }

    /// Whether the entry is a code of the internal stable-key kind.
    pub(crate) fn is_stable_key(&self) -> bool {
        self.class == Class::Code && self.kind == Some(CodeKind::StableKey)
    }
}

/// One contract by which environment configuration or secrets locate a configuration key's record.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) struct Reference {
    /// The contract property: `<path>: <Type>.<member>[.<member>…]`.
    pub source: String,
    /// The accepted decision behind the reference.
    pub decision: String,
    /// Why the contract references the key.
    pub reason: String,
}

impl Entry {
    pub(crate) fn key(&self) -> Key {
        Key {
            path: self.path.clone(),
            class_name: self.class_name.clone(),
            member: self.member.clone(),
        }
    }
}

use std::collections::BTreeMap;
use std::fmt;
use serde::{Serialize, Deserialize};
use crate::constraint::ConstraintSet;
use crate::package::PackageName;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyKind {
    Normal,
    Dev,
    Peer,
    Optional,
}

impl fmt::Display for DependencyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DependencyKind::Normal => write!(f, "normal"),
            DependencyKind::Dev => write!(f, "dev"),
            DependencyKind::Peer => write!(f, "peer"),
            DependencyKind::Optional => write!(f, "optional"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub name: PackageName,
    pub constraint: ConstraintSet,
    pub kind: DependencyKind,
    pub optional: bool,
    pub source: Option<String>,
}

impl Dependency {
    pub fn new(name: PackageName, constraint: ConstraintSet) -> Self {
        Self {
            name,
            constraint,
            kind: DependencyKind::Normal,
            optional: false,
            source: None,
        }
    }

    pub fn with_kind(mut self, kind: DependencyKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.kind != DependencyKind::Normal {
            write!(f, "[{}] ", self.kind)?;
        }
        write!(f, "{} {}", self.name, self.constraint)
    }
}

pub type DependencyMap = BTreeMap<PackageName, Dependency>;

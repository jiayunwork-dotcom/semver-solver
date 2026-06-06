use std::fmt;
use std::str::FromStr;
use serde::{Serialize, Deserialize};
use crate::error::SolverError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PackageName(pub String);

impl PackageName {
    pub fn new(s: &str) -> Self {
        PackageName(s.trim().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for PackageName {
    type Err = SolverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(SolverError::InvalidPackage("Empty package name".to_string()));
        }
        Ok(PackageName(s.trim().to_string()))
    }
}

impl From<String> for PackageName {
    fn from(s: String) -> Self {
        PackageName(s)
    }
}

impl From<&str> for PackageName {
    fn from(s: &str) -> Self {
        PackageName(s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub name: PackageName,
    pub version: crate::version::Version,
    pub dependencies: crate::dependency::DependencyMap,
    pub dev_dependencies: crate::dependency::DependencyMap,
    pub peer_dependencies: crate::dependency::DependencyMap,
    pub optional_dependencies: crate::dependency::DependencyMap,
}

impl Package {
    pub fn new(name: PackageName, version: crate::version::Version) -> Self {
        Self {
            name,
            version,
            dependencies: std::collections::BTreeMap::new(),
            dev_dependencies: std::collections::BTreeMap::new(),
            peer_dependencies: std::collections::BTreeMap::new(),
            optional_dependencies: std::collections::BTreeMap::new(),
        }
    }

    pub fn all_dependencies(&self) -> impl Iterator<Item = (&PackageName, &crate::dependency::Dependency)> {
        self.dependencies.iter()
            .chain(self.peer_dependencies.iter())
    }

    pub fn package_id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

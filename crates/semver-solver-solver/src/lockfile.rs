use std::collections::BTreeMap;
use std::path::Path;
use serde::{Serialize, Deserialize};
use semver_solver_core::{PackageName, Version, error::Result};
use crate::solver::Solution;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFile {
    pub version: u32,
    pub package_manager: String,
    pub packages: BTreeMap<PackageName, LockedPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub version: Version,
    pub integrity: Option<String>,
    pub dependencies: BTreeMap<PackageName, String>,
}

impl LockFile {
    pub fn from_solution(solution: &Solution) -> Self {
        let mut packages = BTreeMap::new();
        for (name, version) in &solution.versions {
            packages.insert(name.clone(), LockedPackage {
                version: version.clone(),
                integrity: None,
                dependencies: BTreeMap::new(),
            });
        }
        Self {
            version: 1,
            package_manager: solution.package_manager.to_string(),
            packages,
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(s: &str) -> Result<Self> {
        Ok(serde_json::from_str(s)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    pub fn to_solution(&self) -> Result<Solution> {
        use std::str::FromStr;
        let mut versions = BTreeMap::new();
        for (name, pkg) in &self.packages {
            versions.insert(name.clone(), pkg.version.clone());
        }
        Ok(Solution {
            versions,
            package_manager: semver_solver_core::PackageManager::from_str(&self.package_manager)?,
            locked: true,
        })
    }
}

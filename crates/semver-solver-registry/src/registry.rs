use std::collections::BTreeMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use semver_solver_core::{PackageName, Version, Package, Dependency, PackageManager, error::Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: Version,
    pub dependencies: BTreeMap<PackageName, String>,
    pub dev_dependencies: BTreeMap<PackageName, String>,
    pub peer_dependencies: BTreeMap<PackageName, String>,
    pub optional_dependencies: BTreeMap<PackageName, String>,
    pub integrity: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub yanked: bool,
}

impl VersionInfo {
    pub fn new(version: Version) -> Self {
        Self {
            version,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            peer_dependencies: BTreeMap::new(),
            optional_dependencies: BTreeMap::new(),
            integrity: None,
            published_at: None,
            yanked: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: PackageName,
    pub versions: Vec<VersionInfo>,
    pub tags: BTreeMap<String, Version>,
}

impl PackageInfo {
    pub fn new(name: PackageName) -> Self {
        Self {
            name,
            versions: Vec::new(),
            tags: BTreeMap::new(),
        }
    }

    pub fn sorted_versions(&self) -> Vec<&VersionInfo> {
        let mut versions: Vec<&VersionInfo> = self.versions.iter()
            .filter(|v| !v.yanked)
            .collect();
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        versions
    }

    pub fn latest_stable(&self) -> Option<&VersionInfo> {
        self.sorted_versions().into_iter()
            .find(|v| v.version.is_stable())
    }

    pub fn latest(&self) -> Option<&VersionInfo> {
        self.sorted_versions().into_iter().next()
    }

    pub fn get_version(&self, v: &Version) -> Option<&VersionInfo> {
        self.versions.iter().find(|vi| &vi.version == v)
    }

    pub fn matching_versions(&self, constraint: &semver_solver_core::ConstraintSet) -> Vec<&VersionInfo> {
        let mut result: Vec<&VersionInfo> = self.versions.iter()
            .filter(|v| !v.yanked && constraint.matches(&v.version))
            .collect();
        result.sort_by(|a, b| b.version.cmp(&a.version));
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackage {
    pub name: PackageName,
    pub version: Version,
    pub dependencies: BTreeMap<PackageName, semver_solver_core::ConstraintSet>,
    pub dev_dependencies: BTreeMap<PackageName, semver_solver_core::ConstraintSet>,
    pub peer_dependencies: BTreeMap<PackageName, semver_solver_core::ConstraintSet>,
    pub optional_dependencies: BTreeMap<PackageName, semver_solver_core::ConstraintSet>,
    pub integrity: Option<String>,
}

impl RegistryPackage {
    pub fn from_version_info(info: &VersionInfo, name: &PackageName, pkg_manager: PackageManager) -> Result<Self> {
        let mut dependencies = BTreeMap::new();
        for (dep_name, constraint_str) in &info.dependencies {
            let constraint = semver_solver_core::ConstraintSet::parse(constraint_str, pkg_manager)?;
            dependencies.insert(dep_name.clone(), constraint);
        }

        let mut dev_dependencies = BTreeMap::new();
        for (dep_name, constraint_str) in &info.dev_dependencies {
            let constraint = semver_solver_core::ConstraintSet::parse(constraint_str, pkg_manager)?;
            dev_dependencies.insert(dep_name.clone(), constraint);
        }

        let mut peer_dependencies = BTreeMap::new();
        for (dep_name, constraint_str) in &info.peer_dependencies {
            let constraint = semver_solver_core::ConstraintSet::parse(constraint_str, pkg_manager)?;
            peer_dependencies.insert(dep_name.clone(), constraint);
        }

        let mut optional_dependencies = BTreeMap::new();
        for (dep_name, constraint_str) in &info.optional_dependencies {
            let constraint = semver_solver_core::ConstraintSet::parse(constraint_str, pkg_manager)?;
            optional_dependencies.insert(dep_name.clone(), constraint);
        }

        Ok(Self {
            name: name.clone(),
            version: info.version.clone(),
            dependencies,
            dev_dependencies,
            peer_dependencies,
            optional_dependencies,
            integrity: info.integrity.clone(),
        })
    }

    pub fn to_package(&self) -> Package {
        let mut pkg = Package::new(self.name.clone(), self.version.clone());
        for (name, constraint) in &self.dependencies {
            let dep = Dependency::new(name.clone(), constraint.clone());
            pkg.dependencies.insert(name.clone(), dep);
        }
        for (name, constraint) in &self.dev_dependencies {
            let dep = Dependency::new(name.clone(), constraint.clone())
                .with_kind(semver_solver_core::DependencyKind::Dev);
            pkg.dev_dependencies.insert(name.clone(), dep);
        }
        for (name, constraint) in &self.peer_dependencies {
            let dep = Dependency::new(name.clone(), constraint.clone())
                .with_kind(semver_solver_core::DependencyKind::Peer);
            pkg.peer_dependencies.insert(name.clone(), dep);
        }
        for (name, constraint) in &self.optional_dependencies {
            let dep = Dependency::new(name.clone(), constraint.clone())
                .with_kind(semver_solver_core::DependencyKind::Optional)
                .with_optional(true);
            pkg.optional_dependencies.insert(name.clone(), dep);
        }
        pkg
    }
}

pub trait Registry: Send + Sync {
    fn package_manager(&self) -> PackageManager;
    fn get_package(&self, name: &PackageName) -> Result<PackageInfo>;
    fn get_package_version(&self, name: &PackageName, version: &Version) -> Result<RegistryPackage>;
    fn list_versions(&self, name: &PackageName) -> Result<Vec<Version>>;
    fn cache_dir(&self) -> Option<PathBuf>;
}

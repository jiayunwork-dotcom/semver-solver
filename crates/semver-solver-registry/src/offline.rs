use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use semver_solver_core::{PackageName, Version, PackageManager, error::Result};
use crate::registry::{Registry, PackageInfo, VersionInfo, RegistryPackage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineRegistry {
    package_manager: PackageManager,
    packages: BTreeMap<PackageName, PackageInfo>,
    cache_dir: Option<PathBuf>,
}

impl OfflineRegistry {
    pub fn new(package_manager: PackageManager) -> Self {
        Self {
            package_manager,
            packages: BTreeMap::new(),
            cache_dir: None,
        }
    }

    pub fn from_file(path: &Path, package_manager: PackageManager) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let raw: BTreeMap<String, Vec<String>> = serde_json::from_str(&content)?;

        let mut packages = BTreeMap::new();
        for (name, versions) in raw {
            let pkg_name = PackageName::new(&name);
            let mut pkg_info = PackageInfo::new(pkg_name.clone());
            for ver_str in versions {
                let version = Version::from_str(&ver_str)?;
                pkg_info.versions.push(VersionInfo::new(version));
            }
            packages.insert(pkg_name, pkg_info);
        }

        Ok(Self {
            package_manager,
            packages,
            cache_dir: None,
        })
    }

    pub fn add_package(&mut self, info: PackageInfo) {
        self.packages.insert(info.name.clone(), info);
    }

    pub fn add_version(&mut self, name: &PackageName, version: VersionInfo) {
        if let Some(pkg) = self.packages.get_mut(name) {
            pkg.versions.push(version);
        } else {
            let mut pkg = PackageInfo::new(name.clone());
            pkg.versions.push(version);
            self.packages.insert(name.clone(), pkg);
        }
    }

    pub fn from_manifest_versions(
        locked: &BTreeMap<PackageName, Version>,
        package_manager: PackageManager,
    ) -> Self {
        let mut registry = Self::new(package_manager);
        for (name, version) in locked {
            registry.add_version(name, VersionInfo::new(version.clone()));
        }
        registry
    }
}

impl Registry for OfflineRegistry {
    fn package_manager(&self) -> PackageManager {
        self.package_manager
    }

    fn get_package(&self, name: &PackageName) -> Result<PackageInfo> {
        self.packages
            .get(name)
            .cloned()
            .ok_or_else(|| semver_solver_core::error::SolverError::PackageNotFound(name.to_string()))
    }

    fn get_package_version(&self, name: &PackageName, version: &Version) -> Result<RegistryPackage> {
        let pkg = self.get_package(name)?;
        let vi = pkg.get_version(version)
            .ok_or_else(|| semver_solver_core::error::SolverError::VersionNotFound(name.to_string(), version.to_string()))?;
        RegistryPackage::from_version_info(vi, name, self.package_manager)
    }

    fn list_versions(&self, name: &PackageName) -> Result<Vec<Version>> {
        let pkg = self.get_package(name)?;
        Ok(pkg.versions.iter().map(|v| v.version.clone()).collect())
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        self.cache_dir.clone()
    }
}

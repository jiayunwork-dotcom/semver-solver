use std::path::PathBuf;
use std::time::Duration;
use std::str::FromStr;
use semver_solver_core::{PackageName, Version, PackageManager, error::{Result, SolverError}};
use crate::cache::RegistryCache;
use crate::registry::{Registry, PackageInfo, VersionInfo, RegistryPackage};

pub struct OnlineRegistry {
    package_manager: PackageManager,
    registry_url: String,
    client: reqwest::blocking::Client,
    cache: Option<RegistryCache>,
    use_cache: bool,
}

impl OnlineRegistry {
    pub fn new(package_manager: PackageManager, registry_url: Option<String>) -> Result<Self> {
        let url = registry_url.unwrap_or_else(|| default_registry_url(package_manager));
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build().map_err(|e| SolverError::HttpError(e.to_string()))?;

        Ok(Self {
            package_manager,
            registry_url: url,
            client,
            cache: None,
            use_cache: true,
        })
    }

    pub fn with_cache(mut self, cache: RegistryCache) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn with_cache_enabled(mut self, enabled: bool) -> Self {
        self.use_cache = enabled;
        self
    }

    fn pm_str(&self) -> &str {
        match self.package_manager {
            PackageManager::Npm => "npm",
            PackageManager::Pip => "pip",
            PackageManager::Cargo => "cargo",
            PackageManager::Go => "go",
        }
    }

    fn fetch_package_npm(&self, name: &PackageName) -> Result<PackageInfo> {
        let url = format!("{}/{}", self.registry_url, name.as_str());
        let resp = self.client.get(&url).send().map_err(|e| SolverError::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(semver_solver_core::error::SolverError::PackageNotFound(name.to_string()));
        }

        let json: serde_json::Value = resp.json().map_err(|e| SolverError::HttpError(e.to_string()))?;
        let mut pkg_info = PackageInfo::new(name.clone());

        if let Some(versions) = json.get("versions").and_then(|v| v.as_object()) {
            for (ver_str, ver_obj) in versions {
                if let Ok(version) = Version::from_str(ver_str) {
                    let mut vi = VersionInfo::new(version);

                    if let Some(deps) = ver_obj.get("dependencies").and_then(|d| d.as_object()) {
                        for (dep_name, dep_constraint) in deps {
                            if let Some(c) = dep_constraint.as_str() {
                                vi.dependencies.insert(PackageName::new(dep_name), c.to_string());
                            }
                        }
                    }
                    if let Some(deps) = ver_obj.get("devDependencies").and_then(|d| d.as_object()) {
                        for (dep_name, dep_constraint) in deps {
                            if let Some(c) = dep_constraint.as_str() {
                                vi.dev_dependencies.insert(PackageName::new(dep_name), c.to_string());
                            }
                        }
                    }
                    if let Some(deps) = ver_obj.get("peerDependencies").and_then(|d| d.as_object()) {
                        for (dep_name, dep_constraint) in deps {
                            if let Some(c) = dep_constraint.as_str() {
                                vi.peer_dependencies.insert(PackageName::new(dep_name), c.to_string());
                            }
                        }
                    }
                    if let Some(deps) = ver_obj.get("optionalDependencies").and_then(|d| d.as_object()) {
                        for (dep_name, dep_constraint) in deps {
                            if let Some(c) = dep_constraint.as_str() {
                                vi.optional_dependencies.insert(PackageName::new(dep_name), c.to_string());
                            }
                        }
                    }

                    if let Some(integrity) = ver_obj.get("dist").and_then(|d| d.get("integrity")).and_then(|i| i.as_str()) {
                        vi.integrity = Some(integrity.to_string());
                    }

                    vi.yanked = ver_obj.get("deprecated").is_some();

                    pkg_info.versions.push(vi);
                }
            }
        }

        if let Some(tags) = json.get("dist-tags").and_then(|t| t.as_object()) {
            for (tag, ver_str) in tags {
                if let Some(v) = ver_str.as_str() {
                    if let Ok(version) = Version::from_str(v) {
                        pkg_info.tags.insert(tag.clone(), version);
                    }
                }
            }
        }

        Ok(pkg_info)
    }

    fn fetch_package(&self, name: &PackageName) -> Result<PackageInfo> {
        match self.package_manager {
            PackageManager::Npm => self.fetch_package_npm(name),
            _ => Err(semver_solver_core::error::SolverError::RegistryError(
                format!("Online registry not implemented for {:?}", self.package_manager)
            )),
        }
    }
}

fn default_registry_url(pm: PackageManager) -> String {
    match pm {
        PackageManager::Npm => "https://registry.npmjs.org".to_string(),
        PackageManager::Pip => "https://pypi.org/pypi".to_string(),
        PackageManager::Cargo => "https://crates.io/api/v1".to_string(),
        PackageManager::Go => "https://proxy.golang.org".to_string(),
    }
}

impl Registry for OnlineRegistry {
    fn package_manager(&self) -> PackageManager {
        self.package_manager
    }

    fn get_package(&self, name: &PackageName) -> Result<PackageInfo> {
        if self.use_cache {
            if let Some(cache) = &self.cache {
                if let Some(cached) = cache.get(self.pm_str(), name) {
                    return Ok(cached);
                }
            }
        }

        let pkg = self.fetch_package(name)?;

        if self.use_cache {
            if let Some(cache) = &self.cache {
                let _ = cache.set(self.pm_str(), name, &pkg);
            }
        }

        Ok(pkg)
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
        self.cache.as_ref().map(|c| c.cache_dir().to_path_buf())
    }
}

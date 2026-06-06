use std::path::Path;
use std::str::FromStr;
use serde_json::Value;
use semver_solver_core::{Package, PackageName, Version, Dependency, DependencyKind, DependencyMap, PackageManager, ConstraintSet, error::Result};
use crate::manifest::{Manifest, ManifestParser};

#[derive(Debug, Clone, Copy)]
pub struct NpmParser;

impl ManifestParser for NpmParser {
    fn package_manager(&self) -> PackageManager {
        PackageManager::Npm
    }

    fn can_parse(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        file_name == "package.json" || file_name == "package-lock.json"
    }

    fn parse(&self, path: &Path) -> Result<Manifest> {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name == "package-lock.json" {
            return self.parse_lock_file(path);
        }

        let content = std::fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&content)?;

        let mut manifest = Manifest::new(PackageManager::Npm, path.to_path_buf());

        if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
            manifest.name = Some(PackageName::new(name));
        }
        if let Some(version) = json.get("version").and_then(|v| v.as_str()) {
            manifest.version = Some(Version::from_str(version)?);
        }

        if let Some(deps) = json.get("dependencies") {
            manifest.dependencies = parse_deps_object(deps, DependencyKind::Normal)?;
        }
        if let Some(deps) = json.get("devDependencies") {
            manifest.dev_dependencies = parse_deps_object(deps, DependencyKind::Dev)?;
        }
        if let Some(deps) = json.get("peerDependencies") {
            manifest.peer_dependencies = parse_deps_object(deps, DependencyKind::Peer)?;
        }
        if let Some(deps) = json.get("optionalDependencies") {
            manifest.optional_dependencies = parse_deps_object(deps, DependencyKind::Optional)?;
        }

        let lock_path = path.with_file_name("package-lock.json");
        if lock_path.exists() {
            if let Ok(locked) = self.parse_lock_file(&lock_path) {
                manifest.locked_versions = locked.locked_versions;
            }
        }

        Ok(manifest)
    }

    fn parse_package(&self, _name: &PackageName, _version: &Version, _path: &Path) -> Result<Package> {
        unimplemented!("NpmParser::parse_package")
    }
}

impl NpmParser {
    fn parse_lock_file(&self, path: &Path) -> Result<Manifest> {
        let content = std::fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&content)?;

        let mut manifest = Manifest::new(PackageManager::Npm, path.to_path_buf());

        if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
            manifest.name = Some(PackageName::new(name));
        }
        if let Some(version) = json.get("version").and_then(|v| v.as_str()) {
            manifest.version = Some(Version::from_str(version)?);
        }

        if let Some(packages) = json.get("packages") {
            if let Some(obj) = packages.as_object() {
                for (key, value) in obj {
                    if key.is_empty() {
                        continue;
                    }
                    let pkg_name = key.trim_start_matches("node_modules/");
                    if let Some(ver) = value.get("version").and_then(|v| v.as_str()) {
                        if let Ok(version) = Version::from_str(ver) {
                            manifest.locked_versions.insert(
                                PackageName::new(pkg_name),
                                version,
                            );
                        }
                    }
                }
            }
        }

        if let Some(deps) = json.get("dependencies") {
            if let Some(obj) = deps.as_object() {
                for (key, value) in obj {
                    if let Some(ver) = value.get("version").and_then(|v| v.as_str()) {
                        if let Ok(version) = Version::from_str(ver) {
                            manifest.locked_versions.insert(
                                PackageName::new(key),
                                version,
                            );
                        }
                    }
                }
            }
        }

        Ok(manifest)
    }
}

fn parse_deps_object(obj: &Value, kind: DependencyKind) -> Result<DependencyMap> {
    let mut deps = DependencyMap::new();
    if let Some(map) = obj.as_object() {
        for (name, constraint_val) in map {
            let constraint_str = constraint_val.as_str().unwrap_or("*");
            let constraint = ConstraintSet::parse(constraint_str, PackageManager::Npm)?;
            let pkg_name = PackageName::new(name);
            let mut dep = Dependency::new(pkg_name.clone(), constraint)
                .with_kind(kind);
            if kind == DependencyKind::Optional {
                dep = dep.with_optional(true);
            }
            deps.insert(pkg_name, dep);
        }
    }
    Ok(deps)
}

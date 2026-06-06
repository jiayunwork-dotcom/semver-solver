use std::path::Path;
use std::str::FromStr;
use semver_solver_core::{Package, PackageName, Version, Dependency, DependencyKind, DependencyMap, PackageManager, ConstraintSet, error::{Result, SolverError}};
use crate::manifest::{Manifest, ManifestParser};

#[derive(Debug, Clone, Copy)]
pub struct CargoParser;

impl ManifestParser for CargoParser {
    fn package_manager(&self) -> PackageManager {
        PackageManager::Cargo
    }

    fn can_parse(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        file_name == "Cargo.toml" || file_name == "Cargo.lock"
    }

    fn parse(&self, path: &Path) -> Result<Manifest> {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name == "Cargo.lock" {
            return self.parse_lock_file(path);
        }

        let content = std::fs::read_to_string(path)?;
        let toml_val: toml::Value = toml::from_str(&content).map_err(|e| SolverError::ParseError(format!("TOML parse error: {}", e)))?;
        let mut manifest = Manifest::new(PackageManager::Cargo, path.to_path_buf());

        if let Some(package) = toml_val.get("package") {
            if let Some(name) = package.get("name").and_then(|v| v.as_str()) {
                manifest.name = Some(PackageName::new(name));
            }
            if let Some(version) = package.get("version").and_then(|v| v.as_str()) {
                manifest.version = Some(Version::from_str(version)?);
            }
        }

        if let Some(deps) = toml_val.get("dependencies") {
            manifest.dependencies = parse_cargo_deps(deps, DependencyKind::Normal)?;
        }
        if let Some(deps) = toml_val.get("dev-dependencies") {
            manifest.dev_dependencies = parse_cargo_deps(deps, DependencyKind::Dev)?;
        }
        if let Some(deps) = toml_val.get("build-dependencies") {
            manifest.optional_dependencies.extend(
                parse_cargo_deps(deps, DependencyKind::Optional)?
            );
        }

        let lock_path = path.with_file_name("Cargo.lock");
        if lock_path.exists() {
            if let Ok(locked) = self.parse_lock_file(&lock_path) {
                manifest.locked_versions = locked.locked_versions;
            }
        }

        Ok(manifest)
    }

    fn parse_package(&self, _name: &PackageName, _version: &Version, _path: &Path) -> Result<Package> {
        unimplemented!("CargoParser::parse_package")
    }
}

impl CargoParser {
    fn parse_lock_file(&self, path: &Path) -> Result<Manifest> {
        let content = std::fs::read_to_string(path)?;
        let toml_val: toml::Value = toml::from_str(&content).map_err(|e| SolverError::ParseError(format!("TOML parse error: {}", e)))?;
        let mut manifest = Manifest::new(PackageManager::Cargo, path.to_path_buf());

        if let Some(packages) = toml_val.get("package").and_then(|v| v.as_array()) {
            for pkg in packages {
                if let (Some(name), Some(version)) = (
                    pkg.get("name").and_then(|v| v.as_str()),
                    pkg.get("version").and_then(|v| v.as_str()),
                ) {
                    if let Ok(ver) = Version::from_str(version) {
                        manifest.locked_versions.insert(
                            PackageName::new(name),
                            ver,
                        );
                    }
                }
            }
        }

        Ok(manifest)
    }
}

fn parse_cargo_deps(obj: &toml::Value, kind: DependencyKind) -> Result<DependencyMap> {
    let mut deps = DependencyMap::new();
    if let Some(table) = obj.as_table() {
        for (name, value) in table {
            let mut constraint_str = String::from("*");
            let mut optional = false;

            match value {
                toml::Value::String(s) => {
                    constraint_str = s.clone();
                }
                toml::Value::Table(t) => {
                    if let Some(v) = t.get("version").and_then(|v| v.as_str()) {
                        constraint_str = v.to_string();
                    }
                    if let Some(opt) = t.get("optional").and_then(|v| v.as_bool()) {
                        optional = opt;
                    }
                }
                _ => continue,
            }

            let constraint = ConstraintSet::parse(&constraint_str, PackageManager::Cargo)?;
            let pkg_name = PackageName::new(name);
            let mut dep = Dependency::new(pkg_name.clone(), constraint).with_kind(kind);
            if optional {
                dep = dep.with_optional(true);
            }
            deps.insert(pkg_name, dep);
        }
    }
    Ok(deps)
}

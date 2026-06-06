use std::path::Path;
use std::str::FromStr;
use regex::Regex;
use once_cell::sync::Lazy;
use semver_solver_core::{Package, PackageName, Version, Dependency, DependencyKind, DependencyMap, PackageManager, ConstraintSet, error::{Result, SolverError}};
use crate::manifest::{Manifest, ManifestParser};

#[derive(Debug, Clone, Copy)]
pub struct PipParser;

impl ManifestParser for PipParser {
    fn package_manager(&self) -> PackageManager {
        PackageManager::Pip
    }

    fn can_parse(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        file_name == "requirements.txt"
            || file_name == "pyproject.toml"
            || file_name == "setup.cfg"
            || file_name == "setup.py"
    }

    fn parse(&self, path: &Path) -> Result<Manifest> {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        match file_name {
            "requirements.txt" => self.parse_requirements_txt(path),
            "pyproject.toml" => self.parse_pyproject_toml(path),
            "setup.cfg" => self.parse_setup_cfg(path),
            _ => {
                let mut manifest = Manifest::new(PackageManager::Pip, path.to_path_buf());
                let req_path = path.with_file_name("requirements.txt");
                if req_path.exists() {
                    let req = self.parse_requirements_txt(&req_path)?;
                    manifest.dependencies = req.dependencies;
                }
                let pyproject_path = path.with_file_name("pyproject.toml");
                if pyproject_path.exists() {
                    let pyproject = self.parse_pyproject_toml(&pyproject_path)?;
                    if manifest.name.is_none() {
                        manifest.name = pyproject.name;
                    }
                    if manifest.version.is_none() {
                        manifest.version = pyproject.version;
                    }
                    if manifest.dependencies.is_empty() {
                        manifest.dependencies = pyproject.dependencies;
                    }
                    manifest.optional_dependencies.extend(pyproject.optional_dependencies);
                }
                Ok(manifest)
            }
        }
    }

    fn parse_package(&self, _name: &PackageName, _version: &Version, _path: &Path) -> Result<Package> {
        unimplemented!("PipParser::parse_package")
    }
}

impl PipParser {
    fn parse_requirements_txt(&self, path: &Path) -> Result<Manifest> {
        let content = std::fs::read_to_string(path)?;
        let mut manifest = Manifest::new(PackageManager::Pip, path.to_path_buf());
        manifest.dependencies = parse_requirements_content(&content, DependencyKind::Normal)?;
        Ok(manifest)
    }

    fn parse_pyproject_toml(&self, path: &Path) -> Result<Manifest> {
        let content = std::fs::read_to_string(path)?;
        let toml_val: toml::Value = toml::from_str(&content).map_err(|e| SolverError::ParseError(format!("TOML parse error: {}", e)))?;
        let mut manifest = Manifest::new(PackageManager::Pip, path.to_path_buf());

        if let Some(project) = toml_val.get("project") {
            if let Some(name) = project.get("name").and_then(|v| v.as_str()) {
                manifest.name = Some(PackageName::new(name));
            }
            if let Some(version) = project.get("version").and_then(|v| v.as_str()) {
                manifest.version = Some(Version::from_str(version)?);
            }
            if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
                for dep in deps {
                    if let Some(dep_str) = dep.as_str() {
                        if let Ok(dep) = parse_pip_dependency(dep_str, DependencyKind::Normal) {
                            manifest.dependencies.insert(dep.name.clone(), dep);
                        }
                    }
                }
            }
            if let Some(optional) = project.get("optional-dependencies").and_then(|v| v.as_table()) {
                for (_, deps) in optional {
                    if let Some(deps_arr) = deps.as_array() {
                        for dep in deps_arr {
                            if let Some(dep_str) = dep.as_str() {
                                if let Ok(dep) = parse_pip_dependency(dep_str, DependencyKind::Optional) {
                                    manifest.optional_dependencies.insert(dep.name.clone(), dep);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(manifest)
    }

    fn parse_setup_cfg(&self, path: &Path) -> Result<Manifest> {
        let content = std::fs::read_to_string(path)?;
        let toml_val: toml::Value = toml::from_str(&content).map_err(|e| SolverError::ParseError(format!("TOML parse error: {}", e)))?;
        let mut manifest = Manifest::new(PackageManager::Pip, path.to_path_buf());

        if let Some(metadata) = toml_val.get("metadata") {
            if let Some(name) = metadata.get("name").and_then(|v| v.as_str()) {
                manifest.name = Some(PackageName::new(name));
            }
            if let Some(version) = metadata.get("version").and_then(|v| v.as_str()) {
                manifest.version = Some(Version::from_str(version)?);
            }
        }

        if let Some(options) = toml_val.get("options") {
            if let Some(install_requires) = options.get("install_requires") {
                if let Some(arr) = install_requires.as_array() {
                    for dep in arr {
                        if let Some(dep_str) = dep.as_str() {
                            if let Ok(dep) = parse_pip_dependency(dep_str, DependencyKind::Normal) {
                                manifest.dependencies.insert(dep.name.clone(), dep);
                            }
                        }
                    }
                } else if let Some(s) = install_requires.as_str() {
                    manifest.dependencies = parse_requirements_content(s, DependencyKind::Normal)?;
                }
            }
        }

        Ok(manifest)
    }
}

fn parse_requirements_content(content: &str, kind: DependencyKind) -> Result<DependencyMap> {
    let mut deps = DependencyMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        if let Ok(dep) = parse_pip_dependency(line, kind) {
            deps.insert(dep.name.clone(), dep);
        }
    }
    Ok(deps)
}

fn parse_pip_dependency(s: &str, kind: DependencyKind) -> Result<Dependency> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"^([A-Za-z0-9_.-]+)\s*(.*)$"#).unwrap()
    });

    let s = s.split('#').next().unwrap_or(s).trim();

    let caps = RE.captures(s).ok_or_else(|| {
        semver_solver_core::error::SolverError::InvalidDependency(s.to_string())
    })?;

    let name = caps.get(1).unwrap().as_str();
    let constraint_str = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");

    let constraint = if constraint_str.is_empty() {
        ConstraintSet::any(PackageManager::Pip)
    } else {
        ConstraintSet::parse(constraint_str, PackageManager::Pip)?
    };

    let pkg_name = PackageName::new(name);
    let mut dep = Dependency::new(pkg_name.clone(), constraint).with_kind(kind);
    if kind == DependencyKind::Optional {
        dep = dep.with_optional(true);
    }

    Ok(dep)
}

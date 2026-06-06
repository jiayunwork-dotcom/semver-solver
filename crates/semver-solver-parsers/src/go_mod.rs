use std::path::Path;
use std::str::FromStr;
use regex::Regex;
use once_cell::sync::Lazy;
use semver_solver_core::{Package, PackageName, Version, Dependency, DependencyKind, DependencyMap, PackageManager, ConstraintSet, error::Result, Prerelease, PrereleaseIdentifier};
use crate::manifest::{Manifest, ManifestParser};

#[derive(Debug, Clone, Copy)]
pub struct GoModParser;

impl ManifestParser for GoModParser {
    fn package_manager(&self) -> PackageManager {
        PackageManager::Go
    }

    fn can_parse(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        file_name == "go.mod" || file_name == "go.sum"
    }

    fn parse(&self, path: &Path) -> Result<Manifest> {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name == "go.sum" {
            return self.parse_sum_file(path);
        }

        let content = std::fs::read_to_string(path)?;
        let mut manifest = Manifest::new(PackageManager::Go, path.to_path_buf());

        static MODULE_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r#"module\s+(\S+)"#).unwrap()
        });
        static GO_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r#"go\s+(\d+\.\d+)"#).unwrap()
        });

        if let Some(caps) = MODULE_RE.captures(&content) {
            manifest.name = Some(PackageName::new(caps.get(1).unwrap().as_str()));
        }

        if let Some(caps) = GO_RE.captures(&content) {
            if let Ok(ver) = Version::from_str(&format!("{}.0", caps.get(1).unwrap().as_str())) {
                manifest.version = Some(ver);
            }
        }

        manifest.dependencies = parse_go_require(&content, DependencyKind::Normal)?;

        let sum_path = path.with_file_name("go.sum");
        if sum_path.exists() {
            if let Ok(sum) = self.parse_sum_file(&sum_path) {
                manifest.locked_versions = sum.locked_versions;
            }
        }

        Ok(manifest)
    }

    fn parse_package(&self, _name: &PackageName, _version: &Version, _path: &Path) -> Result<Package> {
        unimplemented!("GoModParser::parse_package")
    }
}

impl GoModParser {
    fn parse_sum_file(&self, path: &Path) -> Result<Manifest> {
        let content = std::fs::read_to_string(path)?;
        let mut manifest = Manifest::new(PackageManager::Go, path.to_path_buf());

        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0];
                let version_str = parts[1];
                let version_str = version_str.trim_end_matches("/go.mod");
                if let Ok(version) = parse_go_version(version_str) {
                    let pkg_name = PackageName::new(name);
                    if !manifest.locked_versions.contains_key(&pkg_name) {
                        manifest.locked_versions.insert(pkg_name, version);
                    }
                }
            }
        }

        Ok(manifest)
    }
}

fn parse_go_require(content: &str, kind: DependencyKind) -> Result<DependencyMap> {
    let mut deps = DependencyMap::new();

    static REQUIRE_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"require\s*\(([^)]+)\)"#).unwrap()
    });
    static REQUIRE_LINE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"^\s*require\s+(\S+)\s+(\S+)"#).unwrap()
    });
    static DEP_LINE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"^\s*(\S+)\s+(\S+)"#).unwrap()
    });

    for caps in REQUIRE_BLOCK_RE.captures_iter(content) {
        let block_content = caps.get(1).unwrap().as_str();
        for line in block_content.lines() {
            if let Some(dep_caps) = DEP_LINE_RE.captures(line) {
                add_go_dep(&mut deps, dep_caps.get(1).unwrap().as_str(), dep_caps.get(2).unwrap().as_str(), kind);
            }
        }
    }

    for caps in REQUIRE_LINE_RE.captures_iter(content) {
        add_go_dep(&mut deps, caps.get(1).unwrap().as_str(), caps.get(2).unwrap().as_str(), kind);
    }

    Ok(deps)
}

fn add_go_dep(deps: &mut DependencyMap, name: &str, version_str: &str, kind: DependencyKind) {
    let version_str = version_str.trim_end_matches("+incompatible");
    if let Ok(constraint) = ConstraintSet::parse(version_str, PackageManager::Go) {
        let pkg_name = PackageName::new(name);
        let dep = Dependency::new(pkg_name.clone(), constraint).with_kind(kind);
        deps.insert(pkg_name, dep);
    }
}

fn parse_go_version(s: &str) -> Result<Version> {
    let s = s.trim_end_matches("+incompatible");
    if s.starts_with("v0.0.0-") {
        if let Some(rest) = s.strip_prefix("v0.0.0-") {
            let parts: Vec<&str> = rest.splitn(2, '-').collect();
            if parts.len() == 2 {
                let mut v = Version::new(0, 0, 0);
                v.prerelease = Prerelease(vec![
                    PrereleaseIdentifier::AlphaNumeric(parts[0].to_string()),
                    PrereleaseIdentifier::AlphaNumeric(parts[1].to_string()),
                ]);
                v.has_v_prefix = true;
                return Ok(v);
            }
        }
    }
    Version::from_str(s)
}

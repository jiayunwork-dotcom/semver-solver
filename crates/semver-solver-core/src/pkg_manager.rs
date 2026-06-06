use std::fmt;
use std::str::FromStr;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageManager {
    Npm,
    Pip,
    Cargo,
    Go,
}

impl PackageManager {
    pub fn all() -> &'static [PackageManager] {
        &[PackageManager::Npm, PackageManager::Pip, PackageManager::Cargo, PackageManager::Go]
    }

    pub fn default_manifest_files(&self) -> &'static [&'static str] {
        match self {
            PackageManager::Npm => &["package.json", "package-lock.json"],
            PackageManager::Pip => &["requirements.txt", "pyproject.toml", "setup.cfg"],
            PackageManager::Cargo => &["Cargo.toml", "Cargo.lock"],
            PackageManager::Go => &["go.mod"],
        }
    }

    pub fn detect_from_dir(dir: &std::path::Path) -> Option<PackageManager> {
        for pm in PackageManager::all() {
            for file in pm.default_manifest_files() {
                if dir.join(file).exists() {
                    return Some(*pm);
                }
            }
        }
        None
    }
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageManager::Npm => write!(f, "npm"),
            PackageManager::Pip => write!(f, "pip"),
            PackageManager::Cargo => write!(f, "cargo"),
            PackageManager::Go => write!(f, "go"),
        }
    }
}

impl FromStr for PackageManager {
    type Err = crate::error::SolverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "npm" | "node" | "javascript" => Ok(PackageManager::Npm),
            "pip" | "python" | "pypi" => Ok(PackageManager::Pip),
            "cargo" | "rust" | "crates" => Ok(PackageManager::Cargo),
            "go" | "golang" | "gomod" => Ok(PackageManager::Go),
            _ => Err(crate::error::SolverError::UnsupportedPackageManager(s.to_string())),
        }
    }
}

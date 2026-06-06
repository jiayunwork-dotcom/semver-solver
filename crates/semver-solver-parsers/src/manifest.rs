use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use semver_solver_core::{Package, PackageName, Version, Dependency, DependencyMap, PackageManager, error::Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: Option<PackageName>,
    pub version: Option<Version>,
    pub package_manager: PackageManager,
    pub path: PathBuf,
    pub dependencies: DependencyMap,
    pub dev_dependencies: DependencyMap,
    pub peer_dependencies: DependencyMap,
    pub optional_dependencies: DependencyMap,
    pub locked_versions: std::collections::BTreeMap<PackageName, Version>,
}

impl Manifest {
    pub fn new(package_manager: PackageManager, path: PathBuf) -> Self {
        Self {
            name: None,
            version: None,
            package_manager,
            path,
            dependencies: std::collections::BTreeMap::new(),
            dev_dependencies: std::collections::BTreeMap::new(),
            peer_dependencies: std::collections::BTreeMap::new(),
            optional_dependencies: std::collections::BTreeMap::new(),
            locked_versions: std::collections::BTreeMap::new(),
        }
    }

    pub fn all_dependencies(&self) -> impl Iterator<Item = (&PackageName, &Dependency)> {
        self.dependencies.iter()
            .chain(self.peer_dependencies.iter())
    }
}

pub trait ManifestParser {
    fn package_manager(&self) -> PackageManager;
    fn can_parse(&self, path: &Path) -> bool;
    fn parse(&self, path: &Path) -> Result<Manifest>;
    fn parse_package(&self, name: &PackageName, version: &Version, path: &Path) -> Result<Package>;
}

pub fn detect_and_parse(path: &Path) -> Result<Manifest> {
    let parsers: Vec<Box<dyn ManifestParser>> = vec![
        Box::new(crate::npm::NpmParser),
        Box::new(crate::pip::PipParser),
        Box::new(crate::cargo::CargoParser),
        Box::new(crate::go_mod::GoModParser),
    ];

    for parser in parsers {
        if parser.can_parse(path) {
            return parser.parse(path);
        }
    }

    if let Some(pm) = PackageManager::detect_from_dir(path) {
        for parser in vec![
            Box::new(crate::npm::NpmParser) as Box<dyn ManifestParser>,
            Box::new(crate::pip::PipParser),
            Box::new(crate::cargo::CargoParser),
            Box::new(crate::go_mod::GoModParser),
        ] {
            if parser.package_manager() == pm && parser.can_parse(path) {
                return parser.parse(path);
            }
        }
    }

    Err(semver_solver_core::error::SolverError::UnsupportedPackageManager(
        format!("Could not detect package manager for path: {}", path.display())
    ))
}

pub fn get_parser(pm: PackageManager) -> Box<dyn ManifestParser> {
    match pm {
        PackageManager::Npm => Box::new(crate::npm::NpmParser),
        PackageManager::Pip => Box::new(crate::pip::PipParser),
        PackageManager::Cargo => Box::new(crate::cargo::CargoParser),
        PackageManager::Go => Box::new(crate::go_mod::GoModParser),
    }
}

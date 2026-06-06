use std::collections::BTreeMap;
use std::str::FromStr;
use std::path::Path;
use semver_solver_core::{Version, PackageName, PackageManager, error::Result};
use semver_solver_registry::{OfflineRegistry, PackageInfo, VersionInfo};
use semver_solver_parsers::npm::NpmParser;
use semver_solver_parsers::ManifestParser;
use semver_solver_solver::{
    Solver, SolverOptions, SolverResult, DependencyTree, TreeBuildOptions,
    generate_suggestions, diff_versions, what_if_analysis, LockFile,
};

fn create_test_registry_with_deps() -> OfflineRegistry {
    let mut registry = OfflineRegistry::new(PackageManager::Npm);

    let express_ver = Version::from_str("4.18.0").unwrap();
    let express_ver_clone = express_ver.clone();
    let mut express_info = VersionInfo::new(express_ver);
    express_info.dependencies = BTreeMap::from([
        (PackageName::new("body-parser"), "^1.20.0".to_string()),
        (PackageName::new("cookie"), "0.5.0".to_string()),
        (PackageName::new("debug"), "^2.6.9".to_string()),
        (PackageName::new("accepts"), "~1.3.0".to_string()),
    ]);

    let body_parser_ver = Version::from_str("1.20.0").unwrap();
    let body_parser_ver_clone = body_parser_ver.clone();
    let mut body_parser_info = VersionInfo::new(body_parser_ver);
    body_parser_info.dependencies = BTreeMap::from([
        (PackageName::new("qs"), "^6.10.0".to_string()),
        (PackageName::new("debug"), "^2.6.9".to_string()),
    ]);

    let lodash_ver = Version::from_str("4.18.0").unwrap();
    let lodash_info = VersionInfo::new(lodash_ver);

    let debug_ver = Version::from_str("2.6.9").unwrap();
    let debug_info = VersionInfo::new(debug_ver);

    let qs_ver = Version::from_str("6.10.0").unwrap();
    let qs_info = VersionInfo::new(qs_ver);

    let accepts_ver = Version::from_str("1.3.0").unwrap();
    let mut accepts_info = VersionInfo::new(accepts_ver);
    accepts_info.dependencies = BTreeMap::from([
        (PackageName::new("mime-types"), "~2.1.0".to_string()),
        (PackageName::new("negotiator"), "0.6.0".to_string()),
    ]);

    let cookie_ver = Version::from_str("0.5.0").unwrap();
    let cookie_info = VersionInfo::new(cookie_ver);

    let mime_types_ver = Version::from_str("2.1.0").unwrap();
    let mut mime_types_info = VersionInfo::new(mime_types_ver);
    mime_types_info.dependencies = BTreeMap::from([
        (PackageName::new("mime-db"), "1.51.0".to_string()),
    ]);

    let negotiator_ver = Version::from_str("0.6.0").unwrap();
    let negotiator_info = VersionInfo::new(negotiator_ver);

    let mime_db_ver = Version::from_str("1.51.0").unwrap();
    let mime_db_info = VersionInfo::new(mime_db_ver);

    let other_versions = [
        ("express", "4.17.0"),
        ("express", "5.0.0"),
        ("body-parser", "1.19.0"),
        ("body-parser", "2.0.0"),
        ("lodash", "4.17.0"),
        ("debug", "3.0.0"),
        ("debug", "4.0.0"),
        ("qs", "6.9.0"),
        ("qs", "7.0.0"),
        ("accepts", "1.4.0"),
        ("accepts", "2.0.0"),
        ("cookie", "0.4.0"),
        ("negotiator", "0.7.0"),
        ("negotiator", "1.0.0"),
        ("mime-types", "2.2.0"),
        ("mime-types", "3.0.0"),
        ("mime-db", "1.50.0"),
    ];

    for (name, ver) in other_versions {
        let v = Version::from_str(ver).unwrap();
        registry.add_version(&PackageName::new(name), VersionInfo::new(v));
    }

    let express_name = PackageName::new("express");
    let mut express_pkg = PackageInfo::new(express_name.clone());
    for v in ["4.17.0", "4.18.0", "5.0.0"] {
        let ver = Version::from_str(v).unwrap();
        if ver == express_ver_clone {
            express_pkg.versions.push(express_info.clone());
        } else {
            express_pkg.versions.push(VersionInfo::new(ver));
        }
    }
    registry.add_package(express_pkg);

    let bp_name = PackageName::new("body-parser");
    let mut bp_pkg = PackageInfo::new(bp_name.clone());
    for v in ["1.19.0", "1.20.0", "2.0.0"] {
        let ver = Version::from_str(v).unwrap();
        if ver == body_parser_ver_clone {
            bp_pkg.versions.push(body_parser_info.clone());
        } else {
            bp_pkg.versions.push(VersionInfo::new(ver));
        }
    }
    registry.add_package(bp_pkg);

    let lodash_name = PackageName::new("lodash");
    let mut lodash_pkg = PackageInfo::new(lodash_name.clone());
    lodash_pkg.versions.push(VersionInfo::new(Version::from_str("4.17.0").unwrap()));
    lodash_pkg.versions.push(lodash_info);
    registry.add_package(lodash_pkg);

    let debug_name = PackageName::new("debug");
    let mut debug_pkg = PackageInfo::new(debug_name.clone());
    debug_pkg.versions.push(debug_info);
    debug_pkg.versions.push(VersionInfo::new(Version::from_str("3.0.0").unwrap()));
    debug_pkg.versions.push(VersionInfo::new(Version::from_str("4.0.0").unwrap()));
    registry.add_package(debug_pkg);

    let qs_name = PackageName::new("qs");
    let mut qs_pkg = PackageInfo::new(qs_name.clone());
    qs_pkg.versions.push(VersionInfo::new(Version::from_str("6.9.0").unwrap()));
    qs_pkg.versions.push(qs_info);
    qs_pkg.versions.push(VersionInfo::new(Version::from_str("7.0.0").unwrap()));
    registry.add_package(qs_pkg);

    let accepts_name = PackageName::new("accepts");
    let mut accepts_pkg = PackageInfo::new(accepts_name.clone());
    accepts_pkg.versions.push(accepts_info);
    accepts_pkg.versions.push(VersionInfo::new(Version::from_str("1.4.0").unwrap()));
    accepts_pkg.versions.push(VersionInfo::new(Version::from_str("2.0.0").unwrap()));
    registry.add_package(accepts_pkg);

    let cookie_name = PackageName::new("cookie");
    let mut cookie_pkg = PackageInfo::new(cookie_name.clone());
    cookie_pkg.versions.push(VersionInfo::new(Version::from_str("0.4.0").unwrap()));
    cookie_pkg.versions.push(cookie_info);
    registry.add_package(cookie_pkg);

    let mime_name = PackageName::new("mime-types");
    let mut mime_pkg = PackageInfo::new(mime_name.clone());
    mime_pkg.versions.push(mime_types_info);
    mime_pkg.versions.push(VersionInfo::new(Version::from_str("2.2.0").unwrap()));
    mime_pkg.versions.push(VersionInfo::new(Version::from_str("3.0.0").unwrap()));
    registry.add_package(mime_pkg);

    let neg_name = PackageName::new("negotiator");
    let mut neg_pkg = PackageInfo::new(neg_name.clone());
    neg_pkg.versions.push(negotiator_info);
    neg_pkg.versions.push(VersionInfo::new(Version::from_str("0.7.0").unwrap()));
    neg_pkg.versions.push(VersionInfo::new(Version::from_str("1.0.0").unwrap()));
    registry.add_package(neg_pkg);

    let db_name = PackageName::new("mime-db");
    let mut db_pkg = PackageInfo::new(db_name.clone());
    db_pkg.versions.push(VersionInfo::new(Version::from_str("1.50.0").unwrap()));
    db_pkg.versions.push(mime_db_info);
    registry.add_package(db_pkg);

    registry
}

fn create_test_manifest() -> Result<semver_solver_parsers::Manifest> {
    let json = r#"{
        "name": "test-app",
        "version": "1.0.0",
        "dependencies": {
            "express": "^4.17.0",
            "body-parser": "^1.19.0",
            "lodash": "^4.17.0"
        }
    }"#;
    let tmp = std::env::temp_dir().join("test-package.json");
    std::fs::write(&tmp, json)?;
    let result = NpmParser.parse(&tmp);
    std::fs::remove_file(&tmp).ok();
    result
}

fn create_conflict_registry() -> OfflineRegistry {
    let mut registry = OfflineRegistry::new(PackageManager::Npm);

    let a_ver_1_0 = Version::from_str("1.0.0").unwrap();
    let mut a_info_1_0 = VersionInfo::new(a_ver_1_0.clone());
    a_info_1_0.dependencies = BTreeMap::from([
        (PackageName::new("shared-dep"), ">=2.0.0".to_string()),
    ]);

    let a_ver_1_1 = Version::from_str("1.1.0").unwrap();
    let mut a_info_1_1 = VersionInfo::new(a_ver_1_1.clone());
    a_info_1_1.dependencies = BTreeMap::from([
        (PackageName::new("shared-dep"), ">=2.0.0".to_string()),
    ]);

    let b_ver_2_0 = Version::from_str("2.0.0").unwrap();
    let mut b_info_2_0 = VersionInfo::new(b_ver_2_0.clone());
    b_info_2_0.dependencies = BTreeMap::from([
        (PackageName::new("shared-dep"), "<1.5.0".to_string()),
    ]);

    let b_ver_2_1 = Version::from_str("2.1.0").unwrap();
    let mut b_info_2_1 = VersionInfo::new(b_ver_2_1.clone());
    b_info_2_1.dependencies = BTreeMap::from([
        (PackageName::new("shared-dep"), "<1.5.0".to_string()),
    ]);

    for v in ["1.0.0", "1.5.0", "2.0.0", "2.5.0"] {
        let ver = Version::from_str(v).unwrap();
        registry.add_version(&PackageName::new("shared-dep"), VersionInfo::new(ver));
    }

    let a_name = PackageName::new("pkg-a");
    let mut a_pkg = PackageInfo::new(a_name.clone());
    a_pkg.versions.push(a_info_1_0);
    a_pkg.versions.push(a_info_1_1);
    registry.add_package(a_pkg);

    let b_name = PackageName::new("pkg-b");
    let mut b_pkg = PackageInfo::new(b_name.clone());
    b_pkg.versions.push(b_info_2_0);
    b_pkg.versions.push(b_info_2_1);
    registry.add_package(b_pkg);

    registry
}

fn create_conflict_manifest() -> Result<semver_solver_parsers::Manifest> {
    let json = r#"{
        "name": "conflict-app",
        "version": "1.0.0",
        "dependencies": {
            "pkg-a": "^1.0.0",
            "pkg-b": "^2.0.0"
        }
    }"#;
    let tmp = std::env::temp_dir().join("test-conflict-package.json");
    std::fs::write(&tmp, json)?;
    let result = NpmParser.parse(&tmp);
    std::fs::remove_file(&tmp).ok();
    result
}

#[test]
fn test_version_parsing() {
    use semver_solver_core::version::Version;
    use std::str::FromStr;

    let v = Version::from_str("1.2.3").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);

    let v2 = Version::from_str("v2.0.0-beta.1").unwrap();
    assert_eq!(v2.major, 2);
    assert!(v2.has_v_prefix);

    let v3 = Version::from_str("1.2").unwrap();
    assert_eq!(v3.major, 1);
    assert_eq!(v3.minor, 2);
}

#[test]
fn test_version_comparison() {
    use semver_solver_core::version::Version;
    use std::str::FromStr;

    let v1 = Version::from_str("1.2.3").unwrap();
    let v2 = Version::from_str("1.2.4").unwrap();
    let v3 = Version::from_str("1.3.0").unwrap();
    let v4 = Version::from_str("2.0.0").unwrap();

    assert!(v1 < v2);
    assert!(v2 < v3);
    assert!(v3 < v4);

    let pre1 = Version::from_str("1.0.0-alpha").unwrap();
    let pre2 = Version::from_str("1.0.0-beta").unwrap();
    let release = Version::from_str("1.0.0").unwrap();

    assert!(pre1 < pre2);
    assert!(pre2 < release);
}

#[test]
fn test_npm_constraint_caret() {
    use semver_solver_core::constraint::ConstraintSet;
    use semver_solver_core::version::Version;
    use semver_solver_core::PackageManager;
    use std::str::FromStr;

    let c = ConstraintSet::parse("^1.2.3", PackageManager::Npm).unwrap();
    assert!(c.matches(&Version::from_str("1.2.3").unwrap()));
    assert!(c.matches(&Version::from_str("1.9.9").unwrap()));
    assert!(!c.matches(&Version::from_str("2.0.0").unwrap()));

    let c2 = ConstraintSet::parse("^0.2.3", PackageManager::Npm).unwrap();
    assert!(c2.matches(&Version::from_str("0.2.3").unwrap()));
    assert!(c2.matches(&Version::from_str("0.2.9").unwrap()));
    assert!(!c2.matches(&Version::from_str("0.3.0").unwrap()));
}

#[test]
fn test_npm_constraint_tilde() {
    use semver_solver_core::constraint::ConstraintSet;
    use semver_solver_core::version::Version;
    use semver_solver_core::PackageManager;
    use std::str::FromStr;

    let c = ConstraintSet::parse("~1.2.3", PackageManager::Npm).unwrap();
    assert!(c.matches(&Version::from_str("1.2.3").unwrap()));
    assert!(c.matches(&Version::from_str("1.2.9").unwrap()));
    assert!(!c.matches(&Version::from_str("1.3.0").unwrap()));
}

#[test]
fn test_pip_constraint_compatible() {
    use semver_solver_core::constraint::ConstraintSet;
    use semver_solver_core::version::Version;
    use semver_solver_core::PackageManager;
    use std::str::FromStr;

    let c = ConstraintSet::parse("~=1.2", PackageManager::Pip).unwrap();
    assert!(c.matches(&Version::from_str("1.2.0").unwrap()));
    assert!(c.matches(&Version::from_str("1.9.9").unwrap()));
    assert!(!c.matches(&Version::from_str("2.0.0").unwrap()));

    let c2 = ConstraintSet::parse("~=1.2.3", PackageManager::Pip).unwrap();
    assert!(c2.matches(&Version::from_str("1.2.3").unwrap()));
    assert!(c2.matches(&Version::from_str("1.2.9").unwrap()));
    assert!(!c2.matches(&Version::from_str("1.3.0").unwrap()));
}

#[test]
fn test_cargo_constraint_default() {
    use semver_solver_core::constraint::ConstraintSet;
    use semver_solver_core::version::Version;
    use semver_solver_core::PackageManager;
    use std::str::FromStr;

    let c = ConstraintSet::parse("^1.2.3", PackageManager::Cargo).unwrap();
    assert!(c.matches(&Version::from_str("1.2.3").unwrap()));
    assert!(c.matches(&Version::from_str("1.9.9").unwrap()));
    assert!(!c.matches(&Version::from_str("2.0.0").unwrap()));

    let c2 = ConstraintSet::parse("0.2.3", PackageManager::Cargo).unwrap();
    assert!(c2.matches(&Version::from_str("0.2.3").unwrap()));
    assert!(c2.matches(&Version::from_str("0.2.9").unwrap()));
    assert!(!c2.matches(&Version::from_str("0.3.0").unwrap()));
}

#[test]
fn test_dependency_tree_build() -> Result<()> {
    let registry = create_test_registry_with_deps();
    let manifest = create_test_manifest()?;

    let options = TreeBuildOptions::default();
    let tree = DependencyTree::build(&manifest, &registry, options)?;

    let root = &tree.root;
    assert_eq!(root.name.as_str(), "test-app");
    assert!(root.children.len() >= 3);

    let express_child = root.children.iter()
        .find(|c| c.name.as_str() == "express");
    assert!(express_child.is_some());
    assert!(express_child.unwrap().children.len() > 0);

    Ok(())
}

#[test]
fn test_solver_success() -> Result<()> {
    let registry = create_test_registry_with_deps();
    let manifest = create_test_manifest()?;

    let options = SolverOptions::default();
    let mut solver = Solver::new(&registry, &manifest, options);
    let result = solver.solve()?;

    match result {
        SolverResult::Solved(solution) => {
            assert!(solution.versions.contains_key(&PackageName::new("express")));
            assert!(solution.versions.contains_key(&PackageName::new("body-parser")));
            assert!(solution.versions.contains_key(&PackageName::new("lodash")));
            assert!(solution.versions.contains_key(&PackageName::new("debug")));

            let express_ver = solution.versions.get(&PackageName::new("express")).unwrap();
            assert_eq!(express_ver.to_string(), "4.18.0");
        }
        SolverResult::Conflict(_) => panic!("Expected solution, got conflict"),
    }

    Ok(())
}

#[test]
fn test_solver_conflict() -> Result<()> {
    let registry = create_conflict_registry();
    let manifest = create_conflict_manifest()?;

    let options = SolverOptions::default();
    let mut solver = Solver::new(&registry, &manifest, options);
    let result = solver.solve()?;

    match result {
        SolverResult::Solved(_) => panic!("Expected conflict, got solution"),
        SolverResult::Conflict(analysis) => {
            assert!(!analysis.unsatisfiable_core.clauses.is_empty());
            assert!(!analysis.conflict_chains.is_empty());
        }
    }

    Ok(())
}

#[test]
fn test_upgrade_suggestions() -> Result<()> {
    let registry = create_conflict_registry();
    let manifest = create_conflict_manifest()?;

    let options = SolverOptions::default();
    let mut solver = Solver::new(&registry, &manifest, options);
    let result = solver.solve()?;
    let current_solution = match &result {
        SolverResult::Solved(s) => Some(s),
        _ => None,
    };

    let suggestions = generate_suggestions(&solver, current_solution)?;
    println!("Found {} suggestions", suggestions.suggestions.len());

    Ok(())
}

#[test]
fn test_version_diff() -> Result<()> {
    let registry = create_test_registry_with_deps();
    let v1 = Version::from_str("4.17.0")?;
    let v2 = Version::from_str("4.18.0")?;

    let diff = diff_versions(&registry, &PackageName::new("express"), &v1, &v2)?;
    println!("Diff: {:?}", diff);

    Ok(())
}

#[test]
fn test_what_if() -> Result<()> {
    let registry = create_test_registry_with_deps();
    let manifest = create_test_manifest()?;

    let options = SolverOptions::default();
    let upgrade_pkg = PackageName::new("express");
    let upgrade_ver = Version::from_str("5.0.0")?;

    let result = what_if_analysis(&registry, &manifest, upgrade_pkg, upgrade_ver, options)?;
    println!("What-if result: {:?}", result);

    Ok(())
}

#[test]
fn test_lockfile() -> Result<()> {
    let mut versions = BTreeMap::new();
    versions.insert(PackageName::new("express"), Version::from_str("4.18.0")?);
    versions.insert(PackageName::new("lodash"), Version::from_str("4.18.0")?);

    use semver_solver_solver::solver::Solution;
    let solution = Solution {
        versions,
        package_manager: PackageManager::Npm,
        locked: false,
    };

    let lock = LockFile::from_solution(&solution);
    let tmp = std::env::temp_dir().join("test-lock.json");
    lock.save(&tmp)?;

    let loaded = LockFile::load(&tmp)?;
    assert_eq!(loaded.packages.len(), 2);
    assert!(loaded.packages.contains_key(&PackageName::new("express")));
    assert!(loaded.packages.contains_key(&PackageName::new("lodash")));

    std::fs::remove_file(&tmp).ok();
    Ok(())
}

#[test]
fn test_cycle_detection() -> Result<()> {
    let mut registry = OfflineRegistry::new(PackageManager::Npm);

    let a_ver = Version::from_str("1.0.0")?;
    let mut a_info = VersionInfo::new(a_ver.clone());
    a_info.dependencies = BTreeMap::from([
        (PackageName::new("pkg-b"), "^1.0.0".to_string()),
    ]);

    let b_ver = Version::from_str("1.0.0")?;
    let mut b_info = VersionInfo::new(b_ver.clone());
    b_info.dependencies = BTreeMap::from([
        (PackageName::new("pkg-a"), "^1.0.0".to_string()),
    ]);

    let a_name = PackageName::new("pkg-a");
    let mut a_pkg = PackageInfo::new(a_name.clone());
    a_pkg.versions.push(a_info);
    registry.add_package(a_pkg);

    let b_name = PackageName::new("pkg-b");
    let mut b_pkg = PackageInfo::new(b_name.clone());
    b_pkg.versions.push(b_info);
    registry.add_package(b_pkg);

    let json = r#"{
        "name": "cycle-app",
        "version": "1.0.0",
        "dependencies": {
            "pkg-a": "^1.0.0"
        }
    }"#;
    let tmp = std::env::temp_dir().join("test-cycle-package.json");
    std::fs::write(&tmp, json)?;
    let manifest = NpmParser.parse(&tmp)?;
    std::fs::remove_file(&tmp).ok();

    let options = TreeBuildOptions::default();
    let tree = DependencyTree::build(&manifest, &registry, options)?;

    let root = &tree.root;
    let a_child = root.children.iter()
        .find(|c| c.name.as_str() == "pkg-a");
    assert!(a_child.is_some());

    let b_child = a_child.unwrap().children.iter()
        .find(|c| c.name.as_str() == "pkg-b");
    assert!(b_child.is_some());

    let a_grandchild = b_child.unwrap().children.iter()
        .find(|c| c.name.as_str() == "pkg-a");
    assert!(a_grandchild.is_some());
    assert!(a_grandchild.unwrap().circular);

    Ok(())
}

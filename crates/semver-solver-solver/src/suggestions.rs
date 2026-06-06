use serde::{Serialize, Deserialize};
use semver_solver_core::{PackageName, Version, error::Result};
use crate::solver::{Solver, SolverOptions, SolverResult, Solution};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionType {
    Upgrade,
    Downgrade,
    Override,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub suggestion_type: SuggestionType,
    pub package: PackageName,
    pub current_constraint: Option<String>,
    pub suggested_constraint: String,
    pub suggested_version: Version,
    pub impact: Vec<PackageName>,
    pub impact_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeSuggestion {
    pub suggestions: Vec<Suggestion>,
}

pub fn generate_suggestions(
    solver: &Solver,
    current_solution: Option<&Solution>,
) -> Result<UpgradeSuggestion> {
    let mut suggestions = Vec::new();

    let mut root_deps: Vec<(&PackageName, &semver_solver_core::Dependency)> =
        solver.root_deps().iter().collect();
    root_deps.sort_by(|a, b| a.0.cmp(b.0));

    for (name, dep) in root_deps {
        if solver.options().ignores.contains(name) {
            continue;
        }

        let pkg_info = match solver.registry().get_package(name) {
            Ok(pi) => pi,
            Err(_) => continue,
        };

        let all_versions: Vec<Version> = pkg_info.sorted_versions()
            .into_iter()
            .map(|v| v.version.clone())
            .collect();

        for ver in &all_versions {
            if dep.constraint.matches(ver) {
                continue;
            }

            let mut test_options = solver.options().clone();
            test_options.overrides.insert(name.clone(), ver.clone());

            let mut test_solver = Solver::new(
                solver.registry(),
                solver.manifest(),
                test_options,
            );

            if let Ok(SolverResult::Solved(sol)) = test_solver.solve() {
                let impact = compute_impact(current_solution, &sol);

                suggestions.push(Suggestion {
                    suggestion_type: if dep.constraint.matches(&all_versions[0]) {
                        SuggestionType::Downgrade
                    } else {
                        SuggestionType::Upgrade
                    },
                    package: name.clone(),
                    current_constraint: Some(dep.constraint.to_string()),
                    suggested_constraint: format!("^{}", ver),
                    suggested_version: ver.clone(),
                    impact: impact.clone(),
                    impact_count: impact.len(),
                });
            }
        }

        if let Ok(SolverResult::Conflict(_)) = Solver::new(
            solver.registry(),
            solver.manifest(),
            solver.options().clone(),
        ).solve() {
            for ver in &all_versions {
                let mut test_options = solver.options().clone();
                test_options.overrides.insert(name.clone(), ver.clone());

                let mut test_solver = Solver::new(
                    solver.registry(),
                    solver.manifest(),
                    test_options,
                );

                if let Ok(SolverResult::Solved(sol)) = test_solver.solve() {
                    let impact = compute_impact(current_solution, &sol);

                    suggestions.push(Suggestion {
                        suggestion_type: SuggestionType::Override,
                        package: name.clone(),
                        current_constraint: Some(dep.constraint.to_string()),
                        suggested_constraint: format!("=={}", ver),
                        suggested_version: ver.clone(),
                        impact: impact.clone(),
                        impact_count: impact.len(),
                    });
                }
            }
        }
    }

    suggestions.sort_by(|a, b| a.impact_count.cmp(&b.impact_count));

    Ok(UpgradeSuggestion { suggestions })
}

fn compute_impact(current: Option<&Solution>, new: &Solution) -> Vec<PackageName> {
    let mut impact = Vec::new();

    if let Some(current_sol) = current {
        for (name, ver) in &new.versions {
            match current_sol.versions.get(name) {
                Some(current_ver) if current_ver != ver => {
                    impact.push(name.clone());
                }
                None => {
                    impact.push(name.clone());
                }
                _ => {}
            }
        }
        for name in current_sol.versions.keys() {
            if !new.versions.contains_key(name) {
                impact.push(name.clone());
            }
        }
    } else {
        impact.extend(new.versions.keys().cloned());
    }

    impact.sort();
    impact.dedup();
    impact
}

pub fn diff_versions(
    registry: &dyn semver_solver_registry::Registry,
    name: &PackageName,
    v1: &Version,
    v2: &Version,
) -> Result<VersionDiff> {
    let pkg1 = registry.get_package_version(name, v1)?;
    let pkg2 = registry.get_package_version(name, v2)?;

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();

    for (dep_name, constraint) in &pkg2.dependencies {
        match pkg1.dependencies.get(dep_name) {
            Some(old_constraint) if old_constraint == constraint => {
                unchanged.push((dep_name.clone(), constraint.clone()));
            }
            Some(old_constraint) => {
                changed.push((dep_name.clone(), old_constraint.clone(), constraint.clone()));
            }
            None => {
                added.push((dep_name.clone(), constraint.clone()));
            }
        }
    }

    for (dep_name, constraint) in &pkg1.dependencies {
        if !pkg2.dependencies.contains_key(dep_name) {
            removed.push((dep_name.clone(), constraint.clone()));
        }
    }

    Ok(VersionDiff {
        package: name.clone(),
        from_version: v1.clone(),
        to_version: v2.clone(),
        added,
        removed,
        changed,
        unchanged,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub package: PackageName,
    pub from_version: Version,
    pub to_version: Version,
    pub added: Vec<(PackageName, semver_solver_core::ConstraintSet)>,
    pub removed: Vec<(PackageName, semver_solver_core::ConstraintSet)>,
    pub changed: Vec<(PackageName, semver_solver_core::ConstraintSet, semver_solver_core::ConstraintSet)>,
    pub unchanged: Vec<(PackageName, semver_solver_core::ConstraintSet)>,
}

pub fn what_if_analysis(
    registry: &dyn semver_solver_registry::Registry,
    manifest: &semver_solver_parsers::Manifest,
    upgrade_pkg: PackageName,
    upgrade_ver: Version,
    options: SolverOptions,
) -> Result<WhatIfResult> {
    let mut current_solver = Solver::new(registry, manifest, options.clone());
    let current_result = current_solver.solve()?;

    let mut new_options = options.clone();
    new_options.overrides.insert(upgrade_pkg.clone(), upgrade_ver.clone());

    let mut new_solver = Solver::new(registry, manifest, new_options);
    let new_result = new_solver.solve()?;

    match (&current_result, &new_result) {
        (SolverResult::Solved(current), SolverResult::Solved(new)) => {
            let changes = compute_changes(current, new);
            Ok(WhatIfResult::Success {
                changes,
                new_solution: new.clone(),
            })
        }
        (_, SolverResult::Conflict(conflict)) => {
            Ok(WhatIfResult::Conflict {
                conflict: conflict.clone(),
            })
        }
        _ => Ok(WhatIfResult::NoChange),
    }
}

#[derive(Debug, Clone)]
pub enum WhatIfResult {
    Success {
        changes: Vec<PackageChange>,
        new_solution: Solution,
    },
    Conflict {
        conflict: crate::conflict::ConflictAnalysis,
    },
    NoChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageChange {
    pub package: PackageName,
    pub old_version: Option<Version>,
    pub new_version: Option<Version>,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Removed,
    Upgraded,
    Downgraded,
}

fn compute_changes(old: &Solution, new: &Solution) -> Vec<PackageChange> {
    let mut changes = Vec::new();

    for (name, new_ver) in &new.versions {
        match old.versions.get(name) {
            Some(old_ver) if old_ver < new_ver => {
                changes.push(PackageChange {
                    package: name.clone(),
                    old_version: Some(old_ver.clone()),
                    new_version: Some(new_ver.clone()),
                    change_type: ChangeType::Upgraded,
                });
            }
            Some(old_ver) if old_ver > new_ver => {
                changes.push(PackageChange {
                    package: name.clone(),
                    old_version: Some(old_ver.clone()),
                    new_version: Some(new_ver.clone()),
                    change_type: ChangeType::Downgraded,
                });
            }
            None => {
                changes.push(PackageChange {
                    package: name.clone(),
                    old_version: None,
                    new_version: Some(new_ver.clone()),
                    change_type: ChangeType::Added,
                });
            }
            _ => {}
        }
    }

    for (name, old_ver) in &old.versions {
        if !new.versions.contains_key(name) {
            changes.push(PackageChange {
                package: name.clone(),
                old_version: Some(old_ver.clone()),
                new_version: None,
                change_type: ChangeType::Removed,
            });
        }
    }

    changes.sort_by(|a, b| a.package.cmp(&b.package));
    changes
}

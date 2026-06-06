use std::collections::{BTreeMap, HashSet, VecDeque};
use serde::{Serialize, Deserialize};
use semver_solver_core::{PackageName, Version, ConstraintSet, error::Result};
use crate::sat::{SatProblem, Clause, ClauseReason};
use crate::solver::Solver;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictChain {
    pub path: Vec<ConflictStep>,
    pub conflicting_constraint: ConstraintSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictStep {
    pub package: PackageName,
    pub version: Option<Version>,
    pub constraint: ConstraintSet,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsatisfiableCore {
    pub clauses: Vec<ClauseInfo>,
    pub conflicting_package: Option<PackageName>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClauseInfo {
    pub reason: String,
    pub source_package: Option<PackageName>,
    pub source_version: Option<Version>,
    pub target_package: Option<PackageName>,
    pub constraint: Option<ConstraintSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictAnalysis {
    pub unsatisfiable_core: UnsatisfiableCore,
    pub conflict_chains: Vec<ConflictChain>,
    pub conflicting_package: PackageName,
    pub conflicting_constraints: Vec<(ConstraintSet, String)>,
}

pub fn analyze_conflict(problem: &SatProblem, solver: &Solver) -> Result<ConflictAnalysis> {
    let core = extract_unsatisfiable_core(problem, solver)?;
    let chains = find_conflict_chains(problem, solver, &core)?;

    let mut conflicting_package = PackageName::new("unknown");
    let mut conflicting_constraints = Vec::new();

    if let Some(pkg) = &core.conflicting_package {
        conflicting_package = pkg.clone();
        for clause in &core.clauses {
            if let Some(c) = &clause.constraint {
                conflicting_constraints.push((c.clone(), clause.reason.clone()));
            }
        }
    }

    Ok(ConflictAnalysis {
        unsatisfiable_core: core,
        conflict_chains: chains,
        conflicting_package,
        conflicting_constraints,
    })
}

fn extract_unsatisfiable_core(problem: &SatProblem, _solver: &Solver) -> Result<UnsatisfiableCore> {
    let mut clauses_info = Vec::new();
    let mut conflicting_pkg = None;
    let mut pkg_constraint_counts: BTreeMap<PackageName, usize> = BTreeMap::new();

    for clause in &problem.clauses {
        if let Some(reason) = &clause.reason {
            let info = clause_reason_to_info(reason);
            if let Some(pkg) = &info.target_package {
                *pkg_constraint_counts.entry(pkg.clone()).or_insert(0) += 1;
            }
            clauses_info.push(info);
        }
    }

    if let Some((pkg, _)) = pkg_constraint_counts.into_iter().max_by_key(|(_, c)| *c) {
        conflicting_pkg = Some(pkg);
    }

    Ok(UnsatisfiableCore {
        clauses: clauses_info,
        conflicting_package: conflicting_pkg,
    })
}

fn clause_reason_to_info(reason: &ClauseReason) -> ClauseInfo {
    match reason {
        ClauseReason::RootDependency(pkg, constraint) => ClauseInfo {
            reason: format!("Root dependency: {} requires {}", pkg, constraint),
            source_package: None,
            source_version: None,
            target_package: Some(pkg.clone()),
            constraint: Some(constraint.clone()),
        },
        ClauseReason::Dependency(src_pkg, src_ver, target_pkg, constraint) => ClauseInfo {
            reason: format!("{}@{} requires {} {}", src_pkg, src_ver, target_pkg, constraint),
            source_package: Some(src_pkg.clone()),
            source_version: Some(src_ver.clone()),
            target_package: Some(target_pkg.clone()),
            constraint: Some(constraint.clone()),
        },
        ClauseReason::Uniqueness(pkg) => ClauseInfo {
            reason: format!("Package {} must have exactly one version", pkg),
            source_package: None,
            source_version: None,
            target_package: Some(pkg.clone()),
            constraint: None,
        },
        ClauseReason::External(s) => ClauseInfo {
            reason: s.clone(),
            source_package: None,
            source_version: None,
            target_package: None,
            constraint: None,
        },
    }
}

fn find_conflict_chains(
    problem: &SatProblem,
    solver: &Solver,
    core: &UnsatisfiableCore,
) -> Result<Vec<ConflictChain>> {
    let mut chains = Vec::new();

    if let Some(conflict_pkg) = &core.conflicting_package {
        let relevant_clauses: Vec<&Clause> = problem.clauses.iter()
            .filter(|c| {
                if let Some(reason) = &c.reason {
                    match reason {
                        ClauseReason::RootDependency(pkg, _) => pkg == conflict_pkg,
                        ClauseReason::Dependency(_, _, target, _) => target == conflict_pkg,
                        _ => false,
                    }
                } else {
                    false
                }
            })
            .collect();

        let constraint_groups: Vec<(ConstraintSet, Vec<&Clause>)> = relevant_clauses.iter()
            .filter_map(|c| {
                if let Some(reason) = &c.reason {
                    match reason {
                        ClauseReason::RootDependency(_, constraint) => {
                            Some((constraint.clone(), vec![*c]))
                        }
                        ClauseReason::Dependency(_, _, _, constraint) => {
                            Some((constraint.clone(), vec![*c]))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect();

        if constraint_groups.len() >= 2 {
            for i in 0..constraint_groups.len() {
                for j in (i + 1)..constraint_groups.len() {
                    let (c1, clauses1) = &constraint_groups[i];
                    let (c2, _clauses2) = &constraint_groups[j];

                    if constraints_conflict(c1, c2, problem, conflict_pkg) {
                        let chain1 = build_chain(conflict_pkg, c1, &clauses1[0], solver)?;
                        let chain2 = build_chain(conflict_pkg, c2, &problem.clauses[j], solver)?;

                        chains.push(chain1);
                        chains.push(chain2);
                    }
                }
            }
        }

        if chains.is_empty() && !relevant_clauses.is_empty() {
            for clause in relevant_clauses {
                if let Some(reason) = &clause.reason {
                    if let ClauseReason::Dependency(_, _, _, constraint) = reason {
                        let chain = build_chain(conflict_pkg, constraint, clause, solver)?;
                        chains.push(chain);
                    }
                }
            }
        }
    }

    Ok(chains)
}

fn constraints_conflict(
    c1: &ConstraintSet,
    c2: &ConstraintSet,
    problem: &SatProblem,
    pkg: &PackageName,
) -> bool {
    if let Some(vars) = problem.package_vars.get(pkg) {
        for var_id in vars {
            let var_info = &problem.variables[var_id.0];
            let v = &var_info.version;
            if c1.matches(v) && c2.matches(v) {
                return false;
            }
        }
    }
    true
}

fn build_chain(
    target_pkg: &PackageName,
    constraint: &ConstraintSet,
    _clause: &Clause,
    solver: &Solver,
) -> Result<ConflictChain> {
    let mut steps = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    if let Some(root_dep) = solver.root_deps().get(target_pkg) {
        steps.push(ConflictStep {
            package: PackageName::new("root"),
            version: None,
            constraint: root_dep.constraint.clone(),
            source: "root".to_string(),
        });
        steps.push(ConflictStep {
            package: target_pkg.clone(),
            version: None,
            constraint: constraint.clone(),
            source: "root dependency".to_string(),
        });
    } else {
        if let Some(dependents) = solver.collected_deps().get(target_pkg) {
            for (src_pkg, src_ver, dep) in dependents {
                if dep.constraint == *constraint {
                    queue.push_back((src_pkg.clone(), src_ver.clone(), dep.clone(), target_pkg.clone()));
                    break;
                }
            }
        }

        while let Some((pkg, ver, dep, from)) = queue.pop_front() {
            if visited.contains(&pkg) {
                continue;
            }
            visited.insert(pkg.clone());

            steps.insert(0, ConflictStep {
                package: from.clone(),
                version: None,
                constraint: dep.constraint.clone(),
                source: format!("{}@{}", pkg, ver),
            });

            if solver.root_deps().contains_key(&pkg) {
                steps.insert(0, ConflictStep {
                    package: PackageName::new("root"),
                    version: None,
                    constraint: solver.root_deps()[&pkg].constraint.clone(),
                    source: "root".to_string(),
                });
                break;
            }

            if let Some(parents) = solver.collected_deps().get(&pkg) {
                for (src_pkg, src_ver, src_dep) in parents {
                    queue.push_back((src_pkg.clone(), src_ver.clone(), src_dep.clone(), pkg.clone()));
                }
            }
        }
    }

    Ok(ConflictChain {
        path: steps,
        conflicting_constraint: constraint.clone(),
    })
}

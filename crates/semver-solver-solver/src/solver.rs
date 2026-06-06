use std::collections::{BTreeMap, HashSet, VecDeque};
use serde::{Serialize, Deserialize};
use semver_solver_core::{PackageName, Version, Dependency, PackageManager, error::Result};
use semver_solver_registry::Registry;
use semver_solver_parsers::Manifest;
use crate::sat::{SatProblem, SatSolver, SatResult, Lit, Clause, ClauseReason, VarId, build_at_most_one, build_implication};

#[derive(Debug, Clone)]
pub struct SolverOptions {
    pub prefer_latest: bool,
    pub include_dev: bool,
    pub include_optional: bool,
    pub max_decisions: Option<u64>,
    pub overrides: BTreeMap<PackageName, Version>,
    pub ignores: HashSet<PackageName>,
}

impl Default for SolverOptions {
    fn default() -> Self {
        Self {
            prefer_latest: true,
            include_dev: false,
            include_optional: false,
            max_decisions: Some(1_000_000),
            overrides: BTreeMap::new(),
            ignores: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub versions: BTreeMap<PackageName, Version>,
    pub package_manager: PackageManager,
    pub locked: bool,
}

pub struct Solver<'a> {
    registry: &'a dyn Registry,
    options: SolverOptions,
    manifest: &'a Manifest,
    collected_deps: BTreeMap<PackageName, Vec<(PackageName, Version, Dependency)>>,
    root_deps: BTreeMap<PackageName, Dependency>,
}

impl<'a> Solver<'a> {
    pub fn registry(&self) -> &'a dyn Registry {
        self.registry
    }

    pub fn options(&self) -> &SolverOptions {
        &self.options
    }

    pub fn manifest(&self) -> &Manifest {
        self.manifest
    }

    pub fn root_deps(&self) -> &BTreeMap<PackageName, Dependency> {
        &self.root_deps
    }

    pub fn collected_deps(&self) -> &BTreeMap<PackageName, Vec<(PackageName, Version, Dependency)>> {
        &self.collected_deps
    }

    pub fn new(registry: &'a dyn Registry, manifest: &'a Manifest, options: SolverOptions) -> Self {
        Self {
            registry,
            options,
            manifest,
            collected_deps: BTreeMap::new(),
            root_deps: BTreeMap::new(),
        }
    }

    pub fn solve(&mut self) -> Result<SolverResult> {
        if !self.manifest.locked_versions.is_empty() && self.options.prefer_latest {
            let mut all_satisfied = true;
            for (name, dep) in self.manifest.all_dependencies() {
                if let Some(locked) = self.manifest.locked_versions.get(name) {
                    if !dep.constraint.matches(locked) {
                        all_satisfied = false;
                        break;
                    }
                }
            }
            if all_satisfied {
                return Ok(SolverResult::Solved(Solution {
                    versions: self.manifest.locked_versions.clone(),
                    package_manager: self.manifest.package_manager,
                    locked: true,
                }));
            }
        }

        self.collect_all_dependencies()?;
        let problem = self.build_sat_problem()?;
        self.solve_sat(problem)
    }

    fn collect_all_dependencies(&mut self) -> Result<()> {
        for (name, dep) in self.manifest.all_dependencies() {
            if self.options.ignores.contains(name) {
                continue;
            }
            self.root_deps.insert(name.clone(), dep.clone());
        }

        if self.options.include_dev {
            for (name, dep) in &self.manifest.dev_dependencies {
                if self.options.ignores.contains(name) {
                    continue;
                }
                self.root_deps.insert(name.clone(), dep.clone());
            }
        }

        let mut queue: VecDeque<(PackageName, Dependency)> = VecDeque::new();
        for (name, dep) in &self.root_deps {
            queue.push_back((name.clone(), dep.clone()));
        }

        let mut processed: HashSet<PackageName> = HashSet::new();

        while let Some((dep_name, dep)) = queue.pop_front() {
            if self.options.ignores.contains(&dep_name) {
                continue;
            }
            if processed.contains(&dep_name) {
                continue;
            }
            processed.insert(dep_name.clone());

            if let Some(override_ver) = self.options.overrides.get(&dep_name) {
                if let Ok(pkg) = self.registry.get_package_version(&dep_name, override_ver) {
                    let package = pkg.to_package();
                    for (child_name, child_dep) in package.all_dependencies() {
                        if !self.options.include_dev && child_dep.kind == semver_solver_core::DependencyKind::Dev {
                            continue;
                        }
                        if !self.options.include_optional && child_dep.optional {
                            continue;
                        }

                        self.collected_deps
                            .entry(child_name.clone())
                            .or_default()
                            .push((dep_name.clone(), override_ver.clone(), child_dep.clone()));

                        if !processed.contains(&child_name) {
                            queue.push_back((child_name.clone(), child_dep.clone()));
                        }
                    }
                }
                continue;
            }

            let pkg_info = self.registry.get_package(&dep_name)?;
            let matching = pkg_info.matching_versions(&dep.constraint);

            for vi in matching {
                if let Ok(pkg) = self.registry.get_package_version(&dep_name, &vi.version) {
                    let package = pkg.to_package();
                    for (child_name, child_dep) in package.all_dependencies() {
                        if !self.options.include_dev && child_dep.kind == semver_solver_core::DependencyKind::Dev {
                            continue;
                        }
                        if !self.options.include_optional && child_dep.optional {
                            continue;
                        }

                        self.collected_deps
                            .entry(child_name.clone())
                            .or_default()
                            .push((dep_name.clone(), vi.version.clone(), child_dep.clone()));

                        if !processed.contains(&child_name) {
                            queue.push_back((child_name.clone(), child_dep.clone()));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn build_sat_problem(&self) -> Result<SatProblem> {
        let mut problem = SatProblem::new();

        for (name, _) in &self.root_deps {
            if self.options.ignores.contains(name) {
                continue;
            }
            if self.options.overrides.contains_key(name) {
                continue;
            }
            let pkg_info = self.registry.get_package(name)?;
            for vi in &pkg_info.versions {
                if vi.yanked {
                    continue;
                }
                problem.add_variable(name.clone(), vi.version.clone());
            }
        }

        for (name, _) in &self.collected_deps {
            if self.options.ignores.contains(name) {
                continue;
            }
            if self.options.overrides.contains_key(name) {
                continue;
            }
            let pkg_info = self.registry.get_package(name)?;
            for vi in &pkg_info.versions {
                if vi.yanked {
                    continue;
                }
                problem.add_variable(name.clone(), vi.version.clone());
            }
        }

        for (name, dep) in &self.root_deps {
            if self.options.ignores.contains(name) {
                continue;
            }
            if let Some(override_ver) = self.options.overrides.get(name) {
                let var_id = problem.add_variable(name.clone(), override_ver.clone());
                problem.add_clause(Clause::new(
                    vec![Lit::positive(var_id)],
                    Some(ClauseReason::RootDependency(name.clone(), dep.constraint.clone())),
                ));
                continue;
            }

            let pkg_info = self.registry.get_package(name)?;
            let matching: Vec<VarId> = pkg_info.matching_versions(&dep.constraint)
                .into_iter()
                .filter(|v| !v.yanked)
                .map(|v| problem.var_indices[&(name.clone(), v.version.clone())])
                .collect();

            if matching.is_empty() {
                return Err(semver_solver_core::error::SolverError::NoMatchingVersion(
                    name.to_string(),
                    dep.constraint.to_string(),
                ));
            }

            problem.add_clause(Clause::new(
                matching.iter().map(|v| Lit::positive(*v)).collect(),
                Some(ClauseReason::RootDependency(name.clone(), dep.constraint.clone())),
            ));
        }

        let mut uniqueness_clauses = Vec::new();
        for (name, vars) in &problem.package_vars {
            if self.options.ignores.contains(name) {
                continue;
            }
            if self.options.overrides.contains_key(name) {
                continue;
            }
            let clauses = build_at_most_one(vars, ClauseReason::Uniqueness(name.clone()));
            uniqueness_clauses.extend(clauses);
        }
        for clause in uniqueness_clauses {
            problem.add_clause(clause);
        }

        for (dep_name, dependents) in &self.collected_deps {
            if self.options.ignores.contains(dep_name) {
                continue;
            }
            for (parent_name, parent_ver, dep) in dependents {
                let parent_key = (parent_name.clone(), parent_ver.clone());
                if let Some(&parent_var) = problem.var_indices.get(&parent_key) {
                    let constraint = &dep.constraint;

                    if let Some(override_ver) = self.options.overrides.get(dep_name) {
                        if !constraint.matches(override_ver) {
                            problem.add_clause(Clause::new(
                                vec![Lit::negative(parent_var)],
                                Some(ClauseReason::Dependency(
                                    parent_name.clone(),
                                    parent_ver.clone(),
                                    dep_name.clone(),
                                    constraint.clone(),
                                )),
                            ));
                            continue;
                        }
                        let dep_var = problem.add_variable(dep_name.clone(), override_ver.clone());
                        let clause = build_implication(
                            Lit::positive(parent_var),
                            &[Lit::positive(dep_var)],
                            ClauseReason::Dependency(
                                parent_name.clone(),
                                parent_ver.clone(),
                                dep_name.clone(),
                                constraint.clone(),
                            ),
                        );
                        problem.add_clause(clause);
                        continue;
                    }

                    let pkg_info = match self.registry.get_package(dep_name) {
                        Ok(pi) => pi,
                        Err(_) => continue,
                    };
                    let matching: Vec<VarId> = pkg_info.matching_versions(constraint)
                        .into_iter()
                        .filter(|v| !v.yanked)
                        .filter_map(|v| problem.var_indices.get(&(dep_name.clone(), v.version.clone())).copied())
                        .collect();

                    if matching.is_empty() {
                        problem.add_clause(Clause::new(
                            vec![Lit::negative(parent_var)],
                            Some(ClauseReason::Dependency(
                                parent_name.clone(),
                                parent_ver.clone(),
                                dep_name.clone(),
                                constraint.clone(),
                            )),
                        ));
                    } else {
                        let clause = build_implication(
                            Lit::positive(parent_var),
                            &matching.iter().map(|v| Lit::positive(*v)).collect::<Vec<_>>(),
                            ClauseReason::Dependency(
                                parent_name.clone(),
                                parent_ver.clone(),
                                dep_name.clone(),
                                constraint.clone(),
                            ),
                        );
                        problem.add_clause(clause);
                    }
                }
            }
        }

        Ok(problem)
    }

    fn solve_sat(&self, problem: SatProblem) -> Result<SolverResult> {
        let mut solver = SatSolver::new(problem);
        if let Some(max) = self.options.max_decisions {
            solver = solver.with_max_decisions(max);
        }

        let result = solver.solve();
        let problem_ref = solver.get_problem();
        match result {
            SatResult::Satisfiable(assignment) => {
                let mut versions = BTreeMap::new();
                for (i, var) in problem_ref.variables.iter().enumerate() {
                    if assignment[i] == Some(true) {
                        versions.insert(var.package.clone(), var.version.clone());
                    }
                }

                for (name, ver) in &self.options.overrides {
                    versions.insert(name.clone(), ver.clone());
                }

                Ok(SolverResult::Solved(Solution {
                    versions,
                    package_manager: self.manifest.package_manager,
                    locked: false,
                }))
            }
            SatResult::Unsatisfiable => {
                let analysis = crate::conflict::analyze_conflict(
                    problem_ref,
                    self,
                )?;
                Ok(SolverResult::Conflict(analysis))
            }
            SatResult::Unknown => {
                Err(semver_solver_core::error::SolverError::Solver(
                    "Solver timed out or reached max decisions".to_string(),
                ))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SolverResult {
    Solved(Solution),
    Conflict(crate::conflict::ConflictAnalysis),
}

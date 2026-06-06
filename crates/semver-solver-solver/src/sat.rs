use std::collections::{HashSet, BTreeMap};
use std::fmt;
use semver_solver_core::{PackageName, Version, ConstraintSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lit {
    pub var: VarId,
    pub negated: bool,
}

impl Lit {
    pub fn new(var: VarId, negated: bool) -> Self {
        Self { var, negated }
    }

    pub fn positive(var: VarId) -> Self {
        Self { var, negated: false }
    }

    pub fn negative(var: VarId) -> Self {
        Self { var, negated: true }
    }

    pub fn negate(self) -> Self {
        Self { var: self.var, negated: !self.negated }
    }

    pub fn is_true(&self, assignment: &[Option<bool>]) -> bool {
        matches!(assignment[self.var.0], Some(v) if v == !self.negated)
    }

    pub fn is_false(&self, assignment: &[Option<bool>]) -> bool {
        matches!(assignment[self.var.0], Some(v) if v == self.negated)
    }

    pub fn is_unassigned(&self, assignment: &[Option<bool>]) -> bool {
        assignment[self.var.0].is_none()
    }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negated {
            write!(f, "¬")?;
        }
        write!(f, "x{}", self.var.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub lits: Vec<Lit>,
    pub reason: Option<ClauseReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClauseReason {
    RootDependency(PackageName, ConstraintSet),
    Dependency(PackageName, Version, PackageName, ConstraintSet),
    Uniqueness(PackageName),
    External(String),
}

impl Clause {
    pub fn new(lits: Vec<Lit>, reason: Option<ClauseReason>) -> Self {
        Self { lits, reason }
    }

    pub fn is_satisfied(&self, assignment: &[Option<bool>]) -> bool {
        self.lits.iter().any(|l| l.is_true(assignment))
    }

    pub fn is_conflicting(&self, assignment: &[Option<bool>]) -> bool {
        self.lits.iter().all(|l| l.is_false(assignment))
    }

    pub fn is_unit(&self, assignment: &[Option<bool>]) -> Option<Lit> {
        let mut unassigned: Vec<Lit> = Vec::new();
        for lit in &self.lits {
            if lit.is_true(assignment) {
                return None;
            }
            if lit.is_unassigned(assignment) {
                unassigned.push(*lit);
            }
        }
        if unassigned.len() == 1 {
            Some(unassigned[0])
        } else {
            None
        }
    }

    pub fn unassigned_lits<'a>(&'a self, assignment: &'a [Option<bool>]) -> impl Iterator<Item = Lit> + 'a {
        self.lits.iter().copied().filter(move |l| l.is_unassigned(assignment))
    }
}

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub package: PackageName,
    pub version: Version,
}

#[derive(Debug, Clone)]
pub struct SatProblem {
    pub variables: Vec<VariableInfo>,
    pub clauses: Vec<Clause>,
    pub var_indices: BTreeMap<(PackageName, Version), VarId>,
    pub package_vars: BTreeMap<PackageName, Vec<VarId>>,
}

impl SatProblem {
    pub fn new() -> Self {
        Self {
            variables: Vec::new(),
            clauses: Vec::new(),
            var_indices: BTreeMap::new(),
            package_vars: BTreeMap::new(),
        }
    }

    pub fn add_variable(&mut self, package: PackageName, version: Version) -> VarId {
        let key = (package.clone(), version.clone());
        if let Some(&var) = self.var_indices.get(&key) {
            return var;
        }
        let var_id = VarId(self.variables.len());
        self.variables.push(VariableInfo { package: package.clone(), version });
        self.var_indices.insert(key, var_id);
        self.package_vars.entry(package).or_default().push(var_id);
        var_id
    }

    pub fn add_clause(&mut self, clause: Clause) {
        self.clauses.push(clause);
    }

    pub fn num_vars(&self) -> usize {
        self.variables.len()
    }

    pub fn num_clauses(&self) -> usize {
        self.clauses.len()
    }
}

#[derive(Debug, Clone)]
pub struct SatSolver {
    problem: SatProblem,
    assignment: Vec<Option<bool>>,
    trail: Vec<Lit>,
    trail_lim: Vec<usize>,
    reason_clause: Vec<Option<usize>>,
    decision_level: Vec<Option<usize>>,
    watched_lits: Vec<(usize, usize)>,
    conflict_learnt: Vec<Clause>,
    max_decisions: Option<u64>,
    decisions_made: u64,
}

impl SatSolver {
    pub fn new(problem: SatProblem) -> Self {
        let num_vars = problem.num_vars();
        let num_clauses = problem.num_clauses();
        Self {
            problem,
            assignment: vec![None; num_vars],
            trail: Vec::new(),
            trail_lim: Vec::new(),
            reason_clause: vec![None; num_vars],
            decision_level: vec![None; num_vars],
            watched_lits: vec![(0, 1); num_clauses],
            conflict_learnt: Vec::new(),
            max_decisions: None,
            decisions_made: 0,
        }
    }

    pub fn with_max_decisions(mut self, max: u64) -> Self {
        self.max_decisions = Some(max);
        self
    }

    pub fn solve(&mut self) -> SatResult {
        loop {
            match self.unit_propagate() {
                Ok(()) => {
                    if self.all_assigned() {
                        return SatResult::Satisfiable(self.assignment.clone());
                    }
                    if let Some(max) = self.max_decisions {
                        if self.decisions_made >= max {
                            return SatResult::Unknown;
                        }
                    }
                    let lit = self.pick_branching_literal();
                    self.decide(lit);
                }
                Err(conflict_clause_idx) => {
                    if self.decision_level() == 0 {
                        return SatResult::Unsatisfiable;
                    }
                    self.analyze_and_learn(conflict_clause_idx);
                    self.backjump();
                }
            }
        }
    }

    fn unit_propagate(&mut self) -> std::result::Result<(), usize> {
        let mut i = 0;
        while i < self.trail.len() {
            let _lit = self.trail[i];
            i += 1;

            let num_problem_clauses = self.problem.clauses.len();

            let mut assignments: Vec<(Lit, Option<usize>)> = Vec::new();
            let mut conflict: Option<usize> = None;

            for (clause_idx, clause) in self.problem.clauses.iter().enumerate() {
                if clause.is_satisfied(&self.assignment) {
                    continue;
                }
                if clause.is_conflicting(&self.assignment) {
                    conflict = Some(clause_idx);
                    break;
                }
                if let Some(unit_lit) = clause.is_unit(&self.assignment) {
                    if !self.trail.contains(&unit_lit) && !assignments.iter().any(|(l, _)| l == &unit_lit) {
                        assignments.push((unit_lit, Some(clause_idx)));
                    }
                }
            }

            if conflict.is_none() {
                for (clause_idx, clause) in self.conflict_learnt.iter().enumerate() {
                    if clause.is_satisfied(&self.assignment) {
                        continue;
                    }
                    if clause.is_conflicting(&self.assignment) {
                        conflict = Some(num_problem_clauses + clause_idx);
                        break;
                    }
                    if let Some(unit_lit) = clause.is_unit(&self.assignment) {
                        if !self.trail.contains(&unit_lit) && !assignments.iter().any(|(l, _)| l == &unit_lit) {
                            assignments.push((unit_lit, Some(num_problem_clauses + clause_idx)));
                        }
                    }
                }
            }

            if let Some(c) = conflict {
                return Err(c);
            }

            for (lit, reason) in assignments {
                self.assign(lit, reason);
            }
        }
        Ok(())
    }

    fn all_assigned(&self) -> bool {
        self.assignment.iter().all(|a| a.is_some())
    }

    fn pick_branching_literal(&self) -> Lit {
        let unassigned_vars: Vec<VarId> = self.assignment
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_none())
            .map(|(i, _)| VarId(i))
            .collect();

        let mut best_var: Option<(VarId, &Version)> = None;

        for var_id in &unassigned_vars {
            let info = &self.problem.variables[var_id.0];
            let pkg = &info.package;
            let pkg_vars = self.problem.package_vars.get(pkg).unwrap();

            let all_same_package: bool = pkg_vars.iter().all(|vid| {
                self.assignment[vid.0].is_none() || self.assignment[vid.0] == Some(false)
            });

            if all_same_package {
                if best_var.map_or(true, |(_, best_ver)| &info.version > best_ver) {
                    best_var = Some((*var_id, &info.version));
                }
            }
        }

        if let Some((var_id, _)) = best_var {
            return Lit::positive(var_id);
        }

        for var_id in &unassigned_vars {
            let info = &self.problem.variables[var_id.0];
            if best_var.map_or(true, |(_, best_ver)| &info.version > best_ver) {
                best_var = Some((*var_id, &info.version));
            }
        }

        Lit::positive(best_var.unwrap().0)
    }

    fn decide(&mut self, lit: Lit) {
        self.trail_lim.push(self.trail.len());
        self.assign(lit, None);
        self.decisions_made += 1;
    }

    fn assign(&mut self, lit: Lit, reason: Option<usize>) {
        let var = lit.var;
        self.assignment[var.0] = Some(!lit.negated);
        self.reason_clause[var.0] = reason;
        self.decision_level[var.0] = Some(self.decision_level());
        self.trail.push(lit);
    }

    fn decision_level(&self) -> usize {
        self.trail_lim.len()
    }

    fn analyze_and_learn(&mut self, conflict_clause_idx: usize) {
        let all_clauses: Vec<&Clause> = self.problem.clauses.iter()
            .chain(self.conflict_learnt.iter())
            .collect();

        let conflict_clause = &all_clauses[conflict_clause_idx];
        let mut learnt_lits: HashSet<Lit> = conflict_clause.lits.iter().copied().collect();
        let mut seen: HashSet<VarId> = HashSet::new();

        let current_level = self.decision_level();
        let mut last = self.trail.last().copied();

        while let Some(lit) = last {
            if lit.negated {
                learnt_lits.remove(&lit);
                learnt_lits.insert(lit.negate());
            } else {
                learnt_lits.remove(&lit.negate());
                learnt_lits.insert(lit);
            }
            seen.insert(lit.var);

            if let Some(reason_idx) = self.reason_clause[lit.var.0] {
                let reason_clause = &all_clauses[reason_idx];
                for r_lit in &reason_clause.lits {
                    if !seen.contains(&r_lit.var)
                        && self.decision_level[r_lit.var.0].map_or(false, |l| l > 0)
                    {
                        learnt_lits.insert(*r_lit);
                        seen.insert(r_lit.var);
                    }
                }
            }

            let pos = self.trail.iter().rposition(|l| {
                l.var != lit.var
                    && self.decision_level[l.var.0].map_or(false, |l| l == current_level)
            });
            last = pos.map(|p| self.trail[p]);
        }

        let learnt_clause = Clause::new(
            learnt_lits.into_iter().collect(),
            Some(ClauseReason::External("Learned clause".to_string())),
        );
        self.conflict_learnt.push(learnt_clause);
    }

    fn backjump(&mut self) {
        if self.trail_lim.is_empty() {
            return;
        }

        let mut back_level = 0;
        for lit in &self.trail {
            if let Some(level) = self.decision_level[lit.var.0] {
                if level > back_level {
                    back_level = level;
                }
            }
        }

        while self.decision_level() > back_level.saturating_sub(1) {
            if let Some(limit) = self.trail_lim.pop() {
                while self.trail.len() > limit {
                    let lit = self.trail.pop().unwrap();
                    self.assignment[lit.var.0] = None;
                    self.reason_clause[lit.var.0] = None;
                    self.decision_level[lit.var.0] = None;
                }
            }
        }
    }

    pub fn get_assignment(&self) -> &[Option<bool>] {
        &self.assignment
    }

    pub fn get_problem(&self) -> &SatProblem {
        &self.problem
    }
}

#[derive(Debug, Clone)]
pub enum SatResult {
    Satisfiable(Vec<Option<bool>>),
    Unsatisfiable,
    Unknown,
}

pub fn build_at_most_one(vars: &[VarId], reason: ClauseReason) -> Vec<Clause> {
    let mut clauses = Vec::new();
    for i in 0..vars.len() {
        for j in (i + 1)..vars.len() {
            clauses.push(Clause::new(
                vec![Lit::negative(vars[i]), Lit::negative(vars[j])],
                Some(reason.clone()),
            ));
        }
    }
    if !vars.is_empty() {
        let lits: Vec<Lit> = vars.iter().map(|v| Lit::positive(*v)).collect();
        clauses.push(Clause::new(lits, Some(reason)));
    }
    clauses
}

pub fn build_implication(antecedent: Lit, consequents: &[Lit], reason: ClauseReason) -> Clause {
    let mut lits = vec![antecedent.negate()];
    lits.extend(consequents.iter().copied());
    Clause::new(lits, Some(reason))
}

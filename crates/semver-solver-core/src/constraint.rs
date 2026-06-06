use std::fmt;
use std::str::FromStr;
use serde::{Serialize, Deserialize};
use regex::Regex;
use once_cell::sync::Lazy;

use crate::error::SolverError;
use crate::pkg_manager::PackageManager;
use crate::version::{Version, Prerelease, BuildMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op {
    Exact,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    NotEqual,
    Caret,
    Tilde,
    Compatible,
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Constraint {
    pub op: Op,
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstraintSet {
    pub and_constraints: Vec<Vec<Constraint>>,
    pub pkg_manager: PackageManager,
}

impl Constraint {
    pub fn new(op: Op, version: Version) -> Self {
        Self { op, version }
    }

    pub fn matches(&self, v: &Version, pkg_manager: PackageManager) -> bool {
        use Op::*;
        match self.op {
            Exact => v == &self.version,
            Greater => v > &self.version,
            GreaterEq => v >= &self.version,
            Less => v < &self.version,
            LessEq => v <= &self.version,
            NotEqual => v != &self.version,
            Caret => matches_caret(v, &self.version, pkg_manager),
            Tilde => matches_tilde(v, &self.version, pkg_manager),
            Compatible => matches_compatible(v, &self.version),
            Wildcard => true,
        }
    }
}

fn matches_caret(v: &Version, target: &Version, pkg_manager: PackageManager) -> bool {
    if v.is_prerelease() && !target.is_prerelease() {
        return false;
    }
    if v < target {
        return false;
    }

    match pkg_manager {
        PackageManager::Npm => {
            if target.major > 0 {
                v.major == target.major
            } else if target.minor > 0 {
                v.major == target.major && v.minor == target.minor
            } else {
                v.major == target.major && v.minor == target.minor && v.patch == target.patch
            }
        }
        PackageManager::Cargo => {
            if target.major > 0 {
                v.major == target.major
            } else if target.minor > 0 {
                v.major == target.major && v.minor == target.minor
            } else {
                v.major == target.major && v.minor == target.minor && v.patch == target.patch
            }
        }
        PackageManager::Pip | PackageManager::Go => {
            v.major == target.major
        }
    }
}

fn matches_tilde(v: &Version, target: &Version, pkg_manager: PackageManager) -> bool {
    if v.is_prerelease() && !target.is_prerelease() {
        return false;
    }
    if v < target {
        return false;
    }

    match pkg_manager {
        PackageManager::Npm => {
            v.major == target.major && v.minor == target.minor
        }
        PackageManager::Cargo | PackageManager::Pip | PackageManager::Go => {
            v.major == target.major && v.minor == target.minor
        }
    }
}

fn matches_compatible(_v: &Version, _target: &Version) -> bool {
    unreachable!("Compatible operator should be expanded during parsing")
}

impl ConstraintSet {
    pub fn new(pkg_manager: PackageManager) -> Self {
        Self {
            and_constraints: vec![vec![]],
            pkg_manager,
        }
    }

    pub fn any(pkg_manager: PackageManager) -> Self {
        Self {
            and_constraints: vec![vec![Constraint::new(Op::Wildcard, Version::new(0, 0, 0))]],
            pkg_manager,
        }
    }

    pub fn exact(version: Version, pkg_manager: PackageManager) -> Self {
        Self {
            and_constraints: vec![vec![Constraint::new(Op::Exact, version)]],
            pkg_manager,
        }
    }

    pub fn matches(&self, v: &Version) -> bool {
        if self.and_constraints.is_empty() {
            return true;
        }
        self.and_constraints.iter().any(|or_group| {
            or_group.iter().all(|c| c.matches(v, self.pkg_manager))
        })
    }

    pub fn parse(s: &str, pkg_manager: PackageManager) -> Result<Self, SolverError> {
        let s = s.trim();
        if s.is_empty() || s == "*" || s == "latest" {
            return Ok(Self::any(pkg_manager));
        }

        let mut and_constraints = Vec::new();

        for or_part in s.split("||") {
            let or_part = or_part.trim();
            if or_part.is_empty() {
                continue;
            }
            let constraints = parse_constraint_part(or_part, pkg_manager)?;
            and_constraints.push(constraints);
        }

        if and_constraints.is_empty() {
            and_constraints.push(vec![Constraint::new(Op::Wildcard, Version::new(0, 0, 0))]);
        }

        Ok(Self {
            and_constraints,
            pkg_manager,
        })
    }

    pub fn is_exact(&self) -> Option<&Version> {
        if self.and_constraints.len() == 1 && self.and_constraints[0].len() == 1 {
            if self.and_constraints[0][0].op == Op::Exact {
                return Some(&self.and_constraints[0][0].version);
            }
        }
        None
    }
}

fn parse_constraint_part(s: &str, pkg_manager: PackageManager) -> Result<Vec<Constraint>, SolverError> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(==|~=|!=|>=|<=|\^|~|>|<|=)?\s*(v?\d+(?:\.\d+)*(?:\.[xX*])?(?:-[\w.]+)?(?:\+[\w.]+)?)?"#).unwrap()
    });

    let mut constraints = Vec::new();
    let mut last_end = 0;

    for cap in RE.captures_iter(s) {
        let match_start = cap.get(0).unwrap().start();
        if match_start > last_end {
            let between = &s[last_end..match_start].trim();
            if !between.is_empty() && ![" ", ",", "&&"].contains(&between) {
                return Err(SolverError::InvalidConstraint(format!("Unexpected token: {}", between)));
            }
        }
        last_end = cap.get(0).unwrap().end();

        let op_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let ver_str = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        if ver_str.is_empty() && op_str.is_empty() {
            continue;
        }

        if ver_str.contains('x') || ver_str.contains('X') || ver_str.contains('*') {
            let wildcard_constraint = parse_wildcard(ver_str, pkg_manager)?;
            constraints.extend(wildcard_constraint);
            continue;
        }

        let op = match op_str {
            "==" | "=" | "" => Op::Exact,
            ">" => Op::Greater,
            ">=" => Op::GreaterEq,
            "<" => Op::Less,
            "<=" => Op::LessEq,
            "!=" => Op::NotEqual,
            "^" => Op::Caret,
            "~" => Op::Tilde,
            "~=" => Op::Compatible,
            _ => return Err(SolverError::InvalidConstraint(format!("Unknown operator: {}", op_str))),
        };

        let version = if ver_str.is_empty() {
            return Err(SolverError::InvalidConstraint("Missing version".to_string()));
        } else {
            Version::from_str(ver_str)?
        };

        if op == Op::Exact && op_str.is_empty() {
            let contains_dot = ver_str.matches('.').count();
            match pkg_manager {
                PackageManager::Npm => {
                    if contains_dot < 2 {
                        constraints.push(Constraint::new(Op::Caret, version));
                        continue;
                    }
                }
                PackageManager::Cargo => {
                    constraints.push(Constraint::new(Op::Caret, version));
                    continue;
                }
                PackageManager::Pip | PackageManager::Go => {}
            }
        }

        if op == Op::Compatible {
            let dot_count = ver_str.matches('.').count();
            constraints.push(Constraint::new(Op::GreaterEq, version.clone()));
            if dot_count >= 2 {
                let mut upper = version.clone();
                upper.minor += 1;
                upper.patch = 0;
                upper.prerelease = Prerelease::default();
                upper.build = BuildMetadata::default();
                constraints.push(Constraint::new(Op::Less, upper));
            } else {
                let mut upper = version.clone();
                upper.major += 1;
                upper.minor = 0;
                upper.patch = 0;
                upper.prerelease = Prerelease::default();
                upper.build = BuildMetadata::default();
                constraints.push(Constraint::new(Op::Less, upper));
            }
            continue;
        }

        constraints.push(Constraint::new(op, version));
    }

    if constraints.is_empty() {
        return Err(SolverError::InvalidConstraint(format!("No constraints parsed from: {}", s)));
    }

    Ok(constraints)
}

fn parse_wildcard(s: &str, _pkg_manager: PackageManager) -> Result<Vec<Constraint>, SolverError> {
    let parts: Vec<&str> = s.split('.').collect();
    let mut major = 0u64;
    let mut minor = 0u64;

    if parts.len() > 0 && !is_wildcard(parts[0]) {
        major = parts[0].trim_start_matches('v').parse().unwrap_or(0);
    }
    if parts.len() > 1 && !is_wildcard(parts[1]) {
        minor = parts[1].parse().unwrap_or(0);
    }

    let wildcard_pos = parts.iter().position(|p| is_wildcard(p)).unwrap_or(parts.len());

    match wildcard_pos {
        0 => Ok(vec![Constraint::new(Op::Wildcard, Version::new(0, 0, 0))]),
        1 => {
            let lower = Version::new(major, 0, 0);
            let upper = Version::new(major + 1, 0, 0);
            Ok(vec![
                Constraint::new(Op::GreaterEq, lower),
                Constraint::new(Op::Less, upper),
            ])
        }
        _ => {
            let lower = Version::new(major, minor, 0);
            let upper = Version::new(major, minor + 1, 0);
            Ok(vec![
                Constraint::new(Op::GreaterEq, lower),
                Constraint::new(Op::Less, upper),
            ])
        }
    }
}

fn is_wildcard(s: &str) -> bool {
    s == "x" || s == "X" || s == "*" || s.is_empty()
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Exact => write!(f, "=="),
            Op::Greater => write!(f, ">"),
            Op::GreaterEq => write!(f, ">="),
            Op::Less => write!(f, "<"),
            Op::LessEq => write!(f, "<="),
            Op::NotEqual => write!(f, "!="),
            Op::Caret => write!(f, "^"),
            Op::Tilde => write!(f, "~"),
            Op::Compatible => write!(f, "~="),
            Op::Wildcard => write!(f, "*"),
        }
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.op, self.version)
    }
}

impl fmt::Display for ConstraintSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let or_parts: Vec<String> = self.and_constraints
            .iter()
            .map(|and_group| {
                and_group.iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        write!(f, "{}", or_parts.join(" || "))
    }
}

impl FromStr for ConstraintSet {
    type Err = SolverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s, PackageManager::Npm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npm_caret() {
        let cs = ConstraintSet::parse("^1.2.3", PackageManager::Npm).unwrap();
        let v1: Version = "1.2.3".parse().unwrap();
        let v2: Version = "1.9.9".parse().unwrap();
        let v3: Version = "2.0.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }

    #[test]
    fn test_npm_caret_zero() {
        let cs = ConstraintSet::parse("^0.2.3", PackageManager::Npm).unwrap();
        let v1: Version = "0.2.3".parse().unwrap();
        let v2: Version = "0.2.9".parse().unwrap();
        let v3: Version = "0.3.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }

    #[test]
    fn test_npm_tilde() {
        let cs = ConstraintSet::parse("~1.2.3", PackageManager::Npm).unwrap();
        let v1: Version = "1.2.3".parse().unwrap();
        let v2: Version = "1.2.9".parse().unwrap();
        let v3: Version = "1.3.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }

    #[test]
    fn test_pip_compatible() {
        let cs = ConstraintSet::parse("~=1.2", PackageManager::Pip).unwrap();
        let v1: Version = "1.2.0".parse().unwrap();
        let v2: Version = "1.2.9".parse().unwrap();
        let v3: Version = "1.3.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }

    #[test]
    fn test_range() {
        let cs = ConstraintSet::parse(">=1.0.0 <2.0.0", PackageManager::Npm).unwrap();
        let v1: Version = "1.5.0".parse().unwrap();
        let v2: Version = "2.0.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(!cs.matches(&v2));
    }

    #[test]
    fn test_wildcard() {
        let cs = ConstraintSet::parse("1.2.x", PackageManager::Npm).unwrap();
        let v1: Version = "1.2.0".parse().unwrap();
        let v2: Version = "1.2.9".parse().unwrap();
        let v3: Version = "1.3.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }

    #[test]
    fn test_or() {
        let cs = ConstraintSet::parse("^1.0.0 || ^2.0.0", PackageManager::Npm).unwrap();
        let v1: Version = "1.5.0".parse().unwrap();
        let v2: Version = "2.5.0".parse().unwrap();
        let v3: Version = "3.0.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }

    #[test]
    fn test_cargo_default_caret() {
        let cs = ConstraintSet::parse("1.2", PackageManager::Cargo).unwrap();
        let v1: Version = "1.2.0".parse().unwrap();
        let v2: Version = "1.9.9".parse().unwrap();
        let v3: Version = "2.0.0".parse().unwrap();
        assert!(cs.matches(&v1));
        assert!(cs.matches(&v2));
        assert!(!cs.matches(&v3));
    }
}

use std::fmt;
use std::str::FromStr;
use serde::{Serialize, Deserialize};
use crate::error::SolverError;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Prerelease(pub Vec<PrereleaseIdentifier>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildMetadata(pub Vec<String>);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrereleaseIdentifier {
    Numeric(u64),
    AlphaNumeric(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Prerelease,
    pub build: BuildMetadata,
    pub has_v_prefix: bool,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: Prerelease(Vec::new()),
            build: BuildMetadata(Vec::new()),
            has_v_prefix: false,
        }
    }

    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.0.is_empty()
    }

    pub fn is_stable(&self) -> bool {
        self.major > 0 && !self.is_prerelease()
    }

    pub fn to_normalized_string(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if !self.prerelease.0.is_empty() {
            s.push('-');
            s.push_str(&self.prerelease.to_string());
        }
        s
    }
}

impl PartialOrd for PrereleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrereleaseIdentifier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match (self, other) {
            (PrereleaseIdentifier::Numeric(a), PrereleaseIdentifier::Numeric(b)) => a.cmp(b),
            (PrereleaseIdentifier::AlphaNumeric(a), PrereleaseIdentifier::AlphaNumeric(b)) => a.cmp(b),
            (PrereleaseIdentifier::Numeric(_), PrereleaseIdentifier::AlphaNumeric(_)) => Less,
            (PrereleaseIdentifier::AlphaNumeric(_), PrereleaseIdentifier::Numeric(_)) => Greater,
        }
    }
}

impl PartialOrd for Prerelease {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Prerelease {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        if self.0.is_empty() && other.0.is_empty() {
            return Equal;
        }
        if self.0.is_empty() {
            return Greater;
        }
        if other.0.is_empty() {
            return Less;
        }
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            match a.cmp(b) {
                Equal => continue,
                non_eq => return non_eq,
            }
        }
        self.0.len().cmp(&other.0.len())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match self.major.cmp(&other.major) {
            Equal => {}
            non_eq => return non_eq,
        }
        match self.minor.cmp(&other.minor) {
            Equal => {}
            non_eq => return non_eq,
        }
        match self.patch.cmp(&other.patch) {
            Equal => {}
            non_eq => return non_eq,
        }
        self.prerelease.cmp(&other.prerelease)
    }
}

impl fmt::Display for PrereleaseIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrereleaseIdentifier::Numeric(n) => write!(f, "{}", n),
            PrereleaseIdentifier::AlphaNumeric(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Display for Prerelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|p| p.to_string()).collect();
        write!(f, "{}", parts.join("."))
    }
}

impl fmt::Display for BuildMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = if self.has_v_prefix { "v" } else { "" };
        write!(f, "{}{}.{}.{}", prefix, self.major, self.minor, self.patch)?;
        if !self.prerelease.0.is_empty() {
            write!(f, "-{}", self.prerelease)?;
        }
        if !self.build.0.is_empty() {
            write!(f, "+{}", self.build)?;
        }
        Ok(())
    }
}

impl FromStr for Version {
    type Err = SolverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut s = s.trim();
        let has_v_prefix = s.starts_with('v') || s.starts_with('V');
        if has_v_prefix {
            s = &s[1..];
        }

        let (core_str, rest) = match s.split_once('-') {
            Some((c, r)) => (c, Some(r)),
            None => (s, None),
        };

        let (core_str, build_str) = match core_str.split_once('+') {
            Some((c, b)) => (c, Some(b.to_string())),
            None => (core_str, None),
        };

        let mut core_parts = core_str.split('.');
        let major: u64 = core_parts
            .next()
            .ok_or_else(|| SolverError::InvalidVersion(s.to_string()))?
            .parse()
            .map_err(|_| SolverError::InvalidVersion(s.to_string()))?;
        let minor: u64 = core_parts
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|_| SolverError::InvalidVersion(s.to_string()))?;
        let patch: u64 = core_parts
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|_| SolverError::InvalidVersion(s.to_string()))?;

        let (prerelease_str, build_from_rest) = match rest {
            Some(r) => match r.split_once('+') {
                Some((p, b)) => (Some(p), Some(b.to_string())),
                None => (Some(r), None),
            },
            None => (None, None),
        };

        let build = build_str.or(build_from_rest).map(|b| {
            BuildMetadata(b.split('.').map(|s| s.to_string()).collect())
        }).unwrap_or(BuildMetadata(Vec::new()));

        let prerelease = match prerelease_str {
            Some(p) => {
                let mut ids = Vec::new();
                for part in p.split('.') {
                    if let Ok(n) = part.parse::<u64>() {
                        if part.starts_with('0') && part.len() > 1 {
                            ids.push(PrereleaseIdentifier::AlphaNumeric(part.to_string()));
                        } else {
                            ids.push(PrereleaseIdentifier::Numeric(n));
                        }
                    } else {
                        ids.push(PrereleaseIdentifier::AlphaNumeric(part.to_string()));
                    }
                }
                Prerelease(ids)
            }
            None => Prerelease(Vec::new()),
        };

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
            build,
            has_v_prefix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v: Version = "1.2.3".parse().unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.0.is_empty());
    }

    #[test]
    fn test_version_with_v_prefix() {
        let v: Version = "v2.0.0".parse().unwrap();
        assert_eq!(v.major, 2);
        assert!(v.has_v_prefix);
    }

    #[test]
    fn test_version_with_prerelease() {
        let v: Version = "1.0.0-beta.1".parse().unwrap();
        assert_eq!(v.prerelease.0.len(), 2);
        assert!(matches!(v.prerelease.0[0], PrereleaseIdentifier::AlphaNumeric(ref s) if s == "beta"));
        assert!(matches!(v.prerelease.0[1], PrereleaseIdentifier::Numeric(1)));
    }

    #[test]
    fn test_version_ordering() {
        let v1: Version = "1.0.0".parse().unwrap();
        let v2: Version = "1.1.0".parse().unwrap();
        let v3: Version = "1.0.0-alpha".parse().unwrap();
        assert!(v1 < v2);
        assert!(v3 < v1);
    }
}

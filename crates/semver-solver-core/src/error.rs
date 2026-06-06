use thiserror::Error;

pub type Result<T> = std::result::Result<T, SolverError>;

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Invalid constraint: {0}")]
    InvalidConstraint(String),

    #[error("Invalid dependency: {0}")]
    InvalidDependency(String),

    #[error("Invalid package: {0}")]
    InvalidPackage(String),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("No matching version for {0} with constraint {1}")]
    NoMatchingVersion(String, String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("Registry error: {0}")]
    RegistryError(String),

    #[error("Solver error: {0}")]
    Solver(String),

    #[error("Unsupported package manager: {0}")]
    UnsupportedPackageManager(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Conflict detected")]
    ConflictDetected,

    #[error("Unsatisfiable constraints")]
    Unsatisfiable,

    #[error("Other error: {0}")]
    Other(String),
}

impl From<&str> for SolverError {
    fn from(s: &str) -> Self {
        SolverError::Other(s.to_string())
    }
}

impl From<String> for SolverError {
    fn from(s: String) -> Self {
        SolverError::Other(s)
    }
}

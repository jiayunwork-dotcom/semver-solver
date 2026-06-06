pub mod solver;
pub mod sat;
pub mod dep_tree;
pub mod conflict;
pub mod suggestions;
pub mod lockfile;
pub mod output;

pub use solver::{Solver, SolverOptions, SolverResult, Solution};
pub use dep_tree::{DependencyTree, TreeNode, TreeBuildOptions};
pub use conflict::{ConflictAnalysis, ConflictChain, UnsatisfiableCore};
pub use suggestions::{UpgradeSuggestion, Suggestion, VersionDiff, WhatIfResult, PackageChange, diff_versions, what_if_analysis, generate_suggestions};
pub use lockfile::LockFile;
pub use output::{OutputFormat, print_solution, print_conflict, print_tree, print_suggestions, print_diff, print_what_if};

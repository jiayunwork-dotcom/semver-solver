use std::path::{Path, PathBuf};
use std::collections::HashSet;
use clap::{Parser, Subcommand, ValueEnum};
use semver_solver_core::{PackageName, Version, PackageManager, error::Result};
use semver_solver_parsers::{detect_and_parse, get_parser};
use semver_solver_registry::{Registry, OfflineRegistry, OnlineRegistry, RegistryCache};
use semver_solver_solver::{
    Solver, SolverOptions, SolverResult, DependencyTree, TreeBuildOptions,
    OutputFormat, print_solution, print_conflict, print_tree, print_suggestions,
    print_diff, print_what_if, LockFile, generate_suggestions, diff_versions, what_if_analysis,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, global = true)]
    path: Option<PathBuf>,

    #[arg(short, long, global = true, value_enum)]
    format: Option<OutputFormatArg>,

    #[arg(long, global = true, value_enum)]
    package_manager: Option<PackageManagerArg>,

    #[arg(long, global = true)]
    offline: bool,

    #[arg(long, global = true)]
    registry: Option<String>,

    #[arg(long, global = true)]
    versions_file: Option<PathBuf>,

    #[arg(long, global = true, value_delimiter = ',')]
    ignore: Vec<String>,

    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Check {
        #[arg(long, default_value_t = false)]
        include_dev: bool,
        #[arg(long, default_value_t = false)]
        include_optional: bool,
    },
    Tree {
        #[arg(long, default_value_t = 3)]
        depth: usize,
        #[arg(long, default_value_t = false)]
        include_dev: bool,
        #[arg(long, default_value_t = false)]
        include_optional: bool,
    },
    Solve {
        #[arg(long, default_value_t = false)]
        include_dev: bool,
        #[arg(long, default_value_t = false)]
        include_optional: bool,
        #[arg(long)]
        lockfile: Option<PathBuf>,
    },
    Suggest {
        #[arg(long, default_value_t = false)]
        include_dev: bool,
        #[arg(long, default_value_t = false)]
        include_optional: bool,
    },
    Diff {
        package: String,
        from_version: String,
        to_version: String,
    },
    WhatIf {
        #[arg(long)]
        upgrade: Option<String>,
        #[arg(long, default_value_t = false)]
        include_dev: bool,
        #[arg(long, default_value_t = false)]
        include_optional: bool,
    },
    Lock {
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum OutputFormatArg {
    Text,
    Json,
    Dot,
    Html,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum PackageManagerArg {
    Npm,
    Pip,
    Cargo,
    Go,
}

impl From<OutputFormatArg> for OutputFormat {
    fn from(arg: OutputFormatArg) -> Self {
        match arg {
            OutputFormatArg::Text => OutputFormat::Text,
            OutputFormatArg::Json => OutputFormat::Json,
            OutputFormatArg::Dot => OutputFormat::Dot,
            OutputFormatArg::Html => OutputFormat::Html,
        }
    }
}

impl From<PackageManagerArg> for PackageManager {
    fn from(arg: PackageManagerArg) -> Self {
        match arg {
            PackageManagerArg::Npm => PackageManager::Npm,
            PackageManagerArg::Pip => PackageManager::Pip,
            PackageManagerArg::Cargo => PackageManager::Cargo,
            PackageManagerArg::Go => PackageManager::Go,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct ConfigFile {
    registry_url: Option<String>,
    cache_path: Option<PathBuf>,
    cache_ttl_hours: Option<u64>,
    ignore_patterns: Option<Vec<String>>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let path = cli.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let format: OutputFormat = cli.format.unwrap_or(OutputFormatArg::Text).into();
    let format_clone = format.clone();

    let pm = match cli.package_manager {
        Some(pm_arg) => pm_arg.into(),
        None => PackageManager::detect_from_dir(&path)
            .ok_or_else(|| semver_solver_core::error::SolverError::UnsupportedPackageManager(
                "Could not detect package manager. Please specify --package-manager".to_string()
            ))?,
    };

    let config = load_config(cli.config.as_deref())?;

    let mut ignores: HashSet<PackageName> = cli.ignore.iter()
        .map(|s| PackageName::new(s))
        .collect();
    if let Some(patterns) = &config.ignore_patterns {
        for p in patterns {
            ignores.insert(PackageName::new(p));
        }
    }

    let registry = create_registry(&cli, &config, pm)?;

    let manifest_path = find_manifest_path(&path, pm)?;
    let manifest = detect_and_parse(&manifest_path)?;

    let mut solver_options = SolverOptions::default();
    solver_options.ignores = ignores;

    match cli.command {
        Commands::Check { include_dev, include_optional } => {
            solver_options.include_dev = include_dev;
            solver_options.include_optional = include_optional;

            let mut solver = Solver::new(registry.as_ref(), &manifest, solver_options);
            match solver.solve()? {
                SolverResult::Solved(solution) => {
                    print_solution(&solution, format);
                    std::process::exit(0);
                }
                SolverResult::Conflict(analysis) => {
                    print_conflict(&analysis, format);
                    std::process::exit(1);
                }
            }
        }
        Commands::Tree { depth, include_dev, include_optional } => {
            let mut tree_options = TreeBuildOptions::default();
            tree_options.max_depth = depth;
            tree_options.include_dev = include_dev;
            tree_options.include_optional = include_optional;

            let tree = DependencyTree::build(&manifest, registry.as_ref(), tree_options)?;
            print_tree(&tree, Some(depth), format);
        }
        Commands::Solve { include_dev, include_optional, lockfile } => {
            solver_options.include_dev = include_dev;
            solver_options.include_optional = include_optional;

            let mut solver = Solver::new(registry.as_ref(), &manifest, solver_options);
            match solver.solve()? {
                SolverResult::Solved(solution) => {
                    print_solution(&solution, format);

                    if let Some(lock_path) = lockfile {
                        let lock = LockFile::from_solution(&solution);
                        lock.save(&lock_path)?;
                        eprintln!("Lock file saved to: {}", lock_path.display());
                    }
                }
                SolverResult::Conflict(analysis) => {
                    print_conflict(&analysis, format);
                    let suggestions = generate_suggestions(&solver, None)?;
                    print_suggestions(&suggestions, format_clone);
                }
            }
        }
        Commands::Suggest { include_dev, include_optional } => {
            solver_options.include_dev = include_dev;
            solver_options.include_optional = include_optional;

            let mut solver = Solver::new(registry.as_ref(), &manifest, solver_options);
            let result = solver.solve()?;
            let current_solution = match &result {
                SolverResult::Solved(s) => Some(s),
                _ => None,
            };
            let suggestions = generate_suggestions(&solver, current_solution)?;
            print_suggestions(&suggestions, format);
        }
        Commands::Diff { package, from_version, to_version } => {
            use std::str::FromStr;
            let pkg_name = PackageName::new(&package);
            let v1 = Version::from_str(&from_version)?;
            let v2 = Version::from_str(&to_version)?;
            let diff = diff_versions(registry.as_ref(), &pkg_name, &v1, &v2)?;
            print_diff(&diff, format);
        }
        Commands::WhatIf { upgrade, include_dev, include_optional } => {
            solver_options.include_dev = include_dev;
            solver_options.include_optional = include_optional;

            if let Some(upgrade_str) = upgrade {
                use std::str::FromStr;
                let parts: Vec<&str> = upgrade_str.splitn(2, '@').collect();
                if parts.len() != 2 {
                    return Err(semver_solver_core::error::SolverError::InvalidDependency(
                        "Upgrade format should be pkg@version".to_string()
                    ));
                }
                let pkg_name = PackageName::new(parts[0]);
                let ver = Version::from_str(parts[1])?;
                let result = what_if_analysis(registry.as_ref(), &manifest, pkg_name, ver, solver_options)?;
                print_what_if(&result, format);
            }
        }
        Commands::Lock { output } => {
            let mut solver = Solver::new(registry.as_ref(), &manifest, solver_options);
            match solver.solve()? {
                SolverResult::Solved(solution) => {
                    let lock = LockFile::from_solution(&solution);
                    lock.save(&output)?;
                    println!("Lock file saved to: {}", output.display());
                }
                SolverResult::Conflict(analysis) => {
                    print_conflict(&analysis, format);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn load_config(path: Option<&Path>) -> Result<ConfigFile> {
    if let Some(path) = path {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: ConfigFile = toml::from_str(&content).map_err(|e| {
                semver_solver_core::error::SolverError::ParseError(format!("Config parse error: {}", e))
            })?;
            return Ok(config);
        }
    }

    let default_paths = [
        Path::new(".semver-solver.toml"),
        Path::new("semver-solver.toml"),
    ];
    for p in default_paths {
        if p.exists() {
            let content = std::fs::read_to_string(p)?;
            let config: ConfigFile = toml::from_str(&content).map_err(|e| {
                semver_solver_core::error::SolverError::ParseError(format!("Config parse error: {}", e))
            })?;
            return Ok(config);
        }
    }

    Ok(ConfigFile {
        registry_url: None,
        cache_path: None,
        cache_ttl_hours: None,
        ignore_patterns: None,
    })
}

fn create_registry(
    cli: &Cli,
    config: &ConfigFile,
    pm: PackageManager,
) -> Result<Box<dyn Registry>> {
    if cli.offline {
        if let Some(versions_file) = &cli.versions_file {
            let registry = OfflineRegistry::from_file(versions_file, pm)?;
            return Ok(Box::new(registry));
        }

        let parser = get_parser(pm);
        let path = cli.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = find_manifest_path(&path, pm)?;
        let manifest = parser.parse(&manifest_path)?;

        let registry = OfflineRegistry::from_manifest_versions(
            &manifest.locked_versions,
            pm,
        );
        return Ok(Box::new(registry));
    }

    let registry_url = cli.registry.clone()
        .or_else(|| config.registry_url.clone());

    let mut online = OnlineRegistry::new(pm, registry_url)?;

    let cache_dir = config.cache_path.clone()
        .or_else(|| dirs::cache_dir().map(|d| d.join(".semver-cache")))
        .unwrap_or_else(|| PathBuf::from(".semver-cache"));

    let ttl = std::time::Duration::from_secs(
        config.cache_ttl_hours.unwrap_or(24) * 60 * 60
    );

    let cache = RegistryCache::new(cache_dir, ttl)?;
    online = online.with_cache(cache);

    Ok(Box::new(online))
}

fn find_manifest_path(dir: &Path, pm: PackageManager) -> Result<PathBuf> {
    for file in pm.default_manifest_files() {
        let path = dir.join(file);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(semver_solver_core::error::SolverError::ParseError(
        format!("No manifest file found for {:?} in {}", pm, dir.display())
    ))
}

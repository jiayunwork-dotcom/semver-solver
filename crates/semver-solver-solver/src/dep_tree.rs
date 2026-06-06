use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use petgraph::graph::{DiGraph, NodeIndex};
use semver_solver_core::{PackageName, Version, Dependency, DependencyKind, ConstraintSet, PackageManager, error::Result};
use semver_solver_registry::Registry;
use semver_solver_parsers::Manifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: PackageName,
    pub version: Option<Version>,
    pub constraint: Option<ConstraintSet>,
    pub kind: DependencyKind,
    pub children: Vec<TreeNode>,
    pub repeated: bool,
    pub first_occurrence_path: Vec<String>,
    pub circular: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyTree {
    pub root: TreeNode,
    pub package_manager: PackageManager,
    pub all_packages: BTreeMap<PackageName, Vec<Version>>,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TreeBuildOptions {
    pub max_depth: usize,
    pub include_dev: bool,
    pub include_optional: bool,
    pub prefer_locked: bool,
}

impl Default for TreeBuildOptions {
    fn default() -> Self {
        Self {
            max_depth: 3,
            include_dev: false,
            include_optional: false,
            prefer_locked: true,
        }
    }
}

impl DependencyTree {
    pub fn build(
        manifest: &Manifest,
        registry: &dyn Registry,
        options: TreeBuildOptions,
    ) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut all_packages: BTreeMap<PackageName, Vec<Version>> = BTreeMap::new();
        let mut path = Vec::new();

        let root_name = manifest.name.clone().unwrap_or_else(|| PackageName::new("root"));
        let root_version = manifest.version.clone();

        path.push(format!("{}", root_name));

        let mut root = TreeNode {
            name: root_name.clone(),
            version: root_version,
            constraint: None,
            kind: DependencyKind::Normal,
            children: Vec::new(),
            repeated: false,
            first_occurrence_path: path.clone(),
            circular: false,
        };

        let mut visiting = HashSet::new();
        visiting.insert(root_name.clone());

        for (name, dep) in manifest.all_dependencies() {
            let child = build_tree_recursive(
                name,
                dep,
                registry,
                manifest,
                &options,
                1,
                &mut seen,
                &mut visiting,
                &mut all_packages,
                &mut path,
            )?;
            root.children.push(child);
        }

        if options.include_dev {
            for (name, dep) in &manifest.dev_dependencies {
                let child = build_tree_recursive(
                    name,
                    dep,
                    registry,
                    manifest,
                    &options,
                    1,
                    &mut seen,
                    &mut visiting,
                    &mut all_packages,
                    &mut path,
                )?;
                root.children.push(child);
            }
        }

        Ok(Self {
            root,
            package_manager: manifest.package_manager,
            all_packages,
            path: manifest.path.clone(),
        })
    }

    pub fn flatten(&self) -> Vec<(&PackageName, &Option<Version>)> {
        let mut result = Vec::new();
        flatten_tree(&self.root, &mut result);
        result
    }

    pub fn to_graph(&self) -> DiGraph<(PackageName, Option<Version>), ConstraintSet> {
        let mut graph = DiGraph::new();
        let mut node_map = BTreeMap::new();

        let root_idx = add_node(&mut graph, &mut node_map, &self.root.name, &self.root.version);
        build_graph_recursive(&self.root, root_idx, &mut graph, &mut node_map);

        graph
    }

    pub fn print_tree(&self, max_depth: Option<usize>) {
        print_tree_recursive(&self.root, 0, max_depth.unwrap_or(usize::MAX), true, "");
    }
}

fn add_node(
    graph: &mut DiGraph<(PackageName, Option<Version>), ConstraintSet>,
    node_map: &mut BTreeMap<(PackageName, Option<Version>), NodeIndex>,
    name: &PackageName,
    version: &Option<Version>,
) -> NodeIndex {
    let key = (name.clone(), version.clone());
    if let Some(&idx) = node_map.get(&key) {
        return idx;
    }
    let idx = graph.add_node((name.clone(), version.clone()));
    node_map.insert(key, idx);
    idx
}

fn build_graph_recursive(
    node: &TreeNode,
    parent_idx: NodeIndex,
    graph: &mut DiGraph<(PackageName, Option<Version>), ConstraintSet>,
    node_map: &mut BTreeMap<(PackageName, Option<Version>), NodeIndex>,
) {
    for child in &node.children {
        let child_idx = add_node(graph, node_map, &child.name, &child.version);
        if let Some(constraint) = &child.constraint {
            graph.add_edge(parent_idx, child_idx, constraint.clone());
        }
        if !child.repeated && !child.circular {
            build_graph_recursive(child, child_idx, graph, node_map);
        }
    }
}

fn flatten_tree<'a>(node: &'a TreeNode, result: &mut Vec<(&'a PackageName, &'a Option<Version>)>) {
    result.push((&node.name, &node.version));
    for child in &node.children {
        if !child.repeated {
            flatten_tree(child, result);
        }
    }
}

fn print_tree_recursive(
    node: &TreeNode,
    depth: usize,
    max_depth: usize,
    is_last: bool,
    prefix: &str,
) {
    if depth > max_depth {
        return;
    }

    let connector = if is_last { "└── " } else { "├── " };
    let ver_str = node.version
        .as_ref()
        .map(|v| format!("@{}", v))
        .unwrap_or_default();

    let constraint_str = node.constraint
        .as_ref()
        .map(|c| format!(" ({})", c))
        .unwrap_or_default();

    let mut suffix = String::new();
    if node.repeated {
        suffix.push_str(" (repeated)");
    }
    if node.circular {
        suffix.push_str(" (circular)");
    }
    if node.kind != DependencyKind::Normal {
        suffix.push_str(&format!(" [{}]", node.kind));
    }

    println!("{}{}{}{}{}", prefix, connector, node.name, ver_str, constraint_str);

    if !node.repeated && !node.circular {
        let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
        for (i, child) in node.children.iter().enumerate() {
            let is_last_child = i == node.children.len() - 1;
            print_tree_recursive(child, depth + 1, max_depth, is_last_child, &new_prefix);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_tree_recursive(
    name: &PackageName,
    dep: &Dependency,
    registry: &dyn Registry,
    manifest: &Manifest,
    options: &TreeBuildOptions,
    depth: usize,
    seen: &mut HashSet<PackageName>,
    visiting: &mut HashSet<PackageName>,
    all_packages: &mut BTreeMap<PackageName, Vec<Version>>,
    path: &mut Vec<String>,
) -> Result<TreeNode> {
    let current_path = format!("{} -> {}", path.last().unwrap_or(&String::new()), name);
    path.push(current_path.clone());

    if visiting.contains(name) {
        path.pop();
        return Ok(TreeNode {
            name: name.clone(),
            version: None,
            constraint: Some(dep.constraint.clone()),
            kind: dep.kind,
            children: Vec::new(),
            repeated: false,
            first_occurrence_path: path.clone(),
            circular: true,
        });
    }

    if seen.contains(name) && depth > 1 {
        path.pop();
        return Ok(TreeNode {
            name: name.clone(),
            version: None,
            constraint: Some(dep.constraint.clone()),
            kind: dep.kind,
            children: Vec::new(),
            repeated: true,
            first_occurrence_path: path.clone(),
            circular: false,
        });
    }

    seen.insert(name.clone());
    visiting.insert(name.clone());

    let mut selected_version = None;

    if options.prefer_locked {
        if let Some(locked) = manifest.locked_versions.get(name) {
            if dep.constraint.matches(locked) {
                selected_version = Some(locked.clone());
            }
        }
    }

    if selected_version.is_none() {
        if let Ok(pkg_info) = registry.get_package(name) {
            let matching = pkg_info.matching_versions(&dep.constraint);
            if !matching.is_empty() {
                selected_version = Some(matching[0].version.clone());
            }

            all_packages.insert(
                name.clone(),
                pkg_info.versions.iter().map(|v| v.version.clone()).collect(),
            );
        }
    }

    let mut node = TreeNode {
        name: name.clone(),
        version: selected_version.clone(),
        constraint: Some(dep.constraint.clone()),
        kind: dep.kind,
        children: Vec::new(),
        repeated: false,
        first_occurrence_path: path.clone(),
        circular: false,
    };

    if depth < options.max_depth {
        if let Some(version) = &selected_version {
            if let Ok(pkg) = registry.get_package_version(name, version) {
                let package = pkg.to_package();

                for (child_name, child_dep) in package.all_dependencies() {
                    if !options.include_dev && child_dep.kind == DependencyKind::Dev {
                        continue;
                    }
                    if !options.include_optional && child_dep.optional {
                        continue;
                    }
                    if child_dep.kind == DependencyKind::Peer {
                        continue;
                    }

                    let child = build_tree_recursive(
                        &child_name,
                        &child_dep,
                        registry,
                        manifest,
                        options,
                        depth + 1,
                        seen,
                        visiting,
                        all_packages,
                        path,
                    )?;
                    node.children.push(child);
                }
            }
        }
    }

    visiting.remove(name);
    path.pop();

    Ok(node)
}

use serde::Serialize;
use colored::*;
use petgraph::visit::EdgeRef;
use crate::dep_tree::DependencyTree;
use crate::solver::Solution;
use crate::conflict::ConflictAnalysis;
use crate::suggestions::{UpgradeSuggestion, VersionDiff, WhatIfResult, PackageChange, ChangeType};

#[derive(Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
    Dot,
    Html,
}

pub fn print_solution(solution: &Solution, format: OutputFormat) {
    match format {
        OutputFormat::Text => print_solution_text(solution),
        OutputFormat::Json => print_solution_json(solution),
        _ => {}
    }
}

fn print_solution_text(solution: &Solution) {
    println!("{}", "Solution found!".green().bold());
    println!();
    println!("Package manager: {}", solution.package_manager);
    if solution.locked {
        println!("{}", "(Using locked versions)".dimmed());
    }
    println!();
    println!("{}", "Resolved versions:".bold());
    for (name, version) in &solution.versions {
        println!("  {} @ {}", name.to_string().cyan(), version.to_string().yellow());
    }
    println!();
    println!("Total packages: {}", solution.versions.len());
}

fn print_solution_json(solution: &Solution) {
    #[derive(Serialize)]
    struct JsonSolution {
        success: bool,
        package_manager: String,
        locked: bool,
        packages: Vec<JsonPackage>,
    }

    #[derive(Serialize)]
    struct JsonPackage {
        name: String,
        version: String,
    }

    let packages: Vec<JsonPackage> = solution.versions.iter()
        .map(|(name, ver)| JsonPackage {
            name: name.as_str().to_string(),
            version: ver.to_string(),
        })
        .collect();

    let json_sol = JsonSolution {
        success: true,
        package_manager: solution.package_manager.to_string(),
        locked: solution.locked,
        packages,
    };

    println!("{}", serde_json::to_string_pretty(&json_sol).unwrap());
}

pub fn print_conflict(analysis: &ConflictAnalysis, format: OutputFormat) {
    match format {
        OutputFormat::Text => print_conflict_text(analysis),
        OutputFormat::Json => print_conflict_json(analysis),
        _ => {}
    }
}

fn print_conflict_text(analysis: &ConflictAnalysis) {
    println!("{}", "CONFLICT DETECTED".red().bold());
    println!();
    println!("Conflicting package: {}", analysis.conflicting_package.to_string().red().bold());
    println!();

    println!("{}", "Conflicting constraints:".bold());
    for (constraint, source) in &analysis.conflicting_constraints {
        println!("  {}  {}", constraint.to_string().yellow(), format!("(from: {})", source).dimmed());
    }
    println!();

    println!("{}", "Conflict chains:".bold());
    for (i, chain) in analysis.conflict_chains.iter().enumerate() {
        println!();
        println!("Chain {}:", i + 1);
        for (j, step) in chain.path.iter().enumerate() {
            let connector = if j == 0 { "  " } else { "  → " };
            let pkg_str = match &step.version {
                Some(v) => format!("{}@{}", step.package, v),
                None => step.package.to_string(),
            };
            println!("{}{} {}", connector, pkg_str.cyan(), step.constraint.to_string().yellow());
            println!("    {}", format!("required by: {}", step.source).dimmed());
        }
    }
    println!();

    println!("{}", "Minimum unsatisfiable core:".bold());
    for clause in &analysis.unsatisfiable_core.clauses {
        println!("  • {}", clause.reason);
    }
}

fn print_conflict_json(analysis: &ConflictAnalysis) {
    println!("{}", serde_json::to_string_pretty(analysis).unwrap());
}

pub fn print_tree(tree: &DependencyTree, max_depth: Option<usize>, format: OutputFormat) {
    match format {
        OutputFormat::Text => tree.print_tree(max_depth),
        OutputFormat::Json => print_tree_json(tree),
        OutputFormat::Dot => print_tree_dot(tree),
        OutputFormat::Html => print_tree_html(tree),
    }
}

fn print_tree_json(tree: &DependencyTree) {
    println!("{}", serde_json::to_string_pretty(tree).unwrap());
}

fn print_tree_dot(tree: &DependencyTree) {
    let graph = tree.to_graph();
    println!("digraph dependencies {{");
    println!("  node [shape=box, style=filled, fillcolor=lightblue];");

    for idx in graph.node_indices() {
        let (name, version) = &graph[idx];
        let label = match version {
            Some(v) => format!("{}@{}", name, v),
            None => name.to_string(),
        };
        println!("  \"{}\" [label=\"{}\"];", idx.index(), label);
    }

    for edge in graph.edge_references() {
        let constraint = edge.weight();
        println!("  \"{}\" -> \"{}\" [label=\"{}\"];",
            edge.source().index(),
            edge.target().index(),
            constraint.to_string().replace('"', "\\\"")
        );
    }

    println!("}}");
}

fn print_tree_html(tree: &DependencyTree) {
    let graph = tree.to_graph();
    let mut nodes_html = String::new();
    let mut links_html = String::new();

    for idx in graph.node_indices() {
        let (name, version) = &graph[idx];
        let label = match version {
            Some(v) => format!("{}@{}", name, v),
            None => name.to_string(),
        };
        nodes_html.push_str(&format!(
            "{{\"id\": {}, \"name\": \"{}\"}},",
            idx.index(),
            label.replace('"', "\\\"")
        ));
    }

    for edge in graph.edge_references() {
        let constraint = edge.weight();
        links_html.push_str(&format!(
            "{{\"source\": {}, \"target\": {}, \"constraint\": \"{}\"}},",
            edge.source().index(),
            edge.target().index(),
            constraint.to_string().replace('"', "\\\"")
        ));
    }

    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Dependency Graph</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>
        body {{ margin: 0; overflow: hidden; }}
        .node circle {{ fill: #69b3a2; stroke: #fff; stroke-width: 2px; }}
        .node text {{ font: 12px sans-serif; pointer-events: none; }}
        .link {{ stroke: #999; stroke-opacity: 0.6; }}
        .link-label {{ font: 10px sans-serif; fill: #666; }}
    </style>
</head>
<body>
    <svg width="100vw" height="100vh"></svg>
    <script>
        const nodes = [{nodes_html}];
        const links = [{links_html}];

        const svg = d3.select("svg");
        const width = svg.attr("width");
        const height = svg.attr("height");

        const simulation = d3.forceSimulation(nodes)
            .force("link", d3.forceLink(links).id(d => d.id).distance(100))
            .force("charge", d3.forceManyBody().strength(-300))
            .force("center", d3.forceCenter(width / 2, height / 2));

        const link = svg.append("g")
            .selectAll("line")
            .data(links)
            .join("line")
            .attr("class", "link")
            .attr("stroke-width", 1.5);

        const linkLabel = svg.append("g")
            .selectAll("text")
            .data(links)
            .join("text")
            .attr("class", "link-label")
            .text(d => d.constraint);

        const node = svg.append("g")
            .selectAll("g")
            .data(nodes)
            .join("g")
            .attr("class", "node")
            .call(d3.drag()
                .on("start", dragstarted)
                .on("drag", dragged)
                .on("end", dragended));

        node.append("circle")
            .attr("r", 20);

        node.append("text")
            .attr("dy", -25)
            .attr("text-anchor", "middle")
            .text(d => d.name);

        simulation.on("tick", () => {{
            link
                .attr("x1", d => d.source.x)
                .attr("y1", d => d.source.y)
                .attr("x2", d => d.target.x)
                .attr("y2", d => d.target.y);

            linkLabel
                .attr("x", d => (d.source.x + d.target.x) / 2)
                .attr("y", d => (d.source.y + d.target.y) / 2);

            node.attr("transform", d => `translate(${{d.x}}, ${{d.y}})`);
        }});

        function dragstarted(event, d) {{
            if (!event.active) simulation.alphaTarget(0.3).restart();
            d.fx = d.x;
            d.fy = d.y;
        }}

        function dragged(event, d) {{
            d.fx = event.x;
            d.fy = event.y;
        }}

        function dragended(event, d) {{
            if (!event.active) simulation.alphaTarget(0);
            d.fx = null;
            d.fy = null;
        }}
    </script>
</body>
</html>"#);

    println!("{}", html);
}

pub fn print_suggestions(suggestions: &UpgradeSuggestion, format: OutputFormat) {
    match format {
        OutputFormat::Text => print_suggestions_text(suggestions),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(suggestions).unwrap()),
        _ => {}
    }
}

fn print_suggestions_text(suggestions: &UpgradeSuggestion) {
    println!("{}", "Suggestions:".green().bold());
    println!();

    if suggestions.suggestions.is_empty() {
        println!("{}", "No suggestions available.".dimmed());
        return;
    }

    for (i, suggestion) in suggestions.suggestions.iter().enumerate() {
        let type_str = match suggestion.suggestion_type {
            crate::suggestions::SuggestionType::Upgrade => "UPGRADE".green().to_string(),
            crate::suggestions::SuggestionType::Downgrade => "DOWNGRADE".yellow().to_string(),
            crate::suggestions::SuggestionType::Override => "OVERRIDE".magenta().to_string(),
        };

        println!("{}. [{}] {}: {} → {}",
            i + 1,
            type_str,
            suggestion.package.to_string().cyan(),
            suggestion.current_constraint.as_deref().unwrap_or("*"),
            suggestion.suggested_constraint.yellow()
        );
        println!("   Impact: {} packages affected", suggestion.impact_count);
        if !suggestion.impact.is_empty() {
            println!("   Affected packages: {}",
                suggestion.impact.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "));
        }
        println!();
    }
}

pub fn print_diff(diff: &VersionDiff, format: OutputFormat) {
    match format {
        OutputFormat::Text => print_diff_text(diff),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(diff).unwrap()),
        _ => {}
    }
}

fn print_diff_text(diff: &VersionDiff) {
    println!("{}", format!("Dependency diff: {}@{} → {}@{}",
        diff.package, diff.from_version, diff.package, diff.to_version).bold());
    println!();

    if !diff.added.is_empty() {
        println!("{}", "Added dependencies:".green().bold());
        for (name, constraint) in &diff.added {
            println!("  + {} {}", name, constraint);
        }
        println!();
    }

    if !diff.removed.is_empty() {
        println!("{}", "Removed dependencies:".red().bold());
        for (name, constraint) in &diff.removed {
            println!("  - {} {}", name, constraint);
        }
        println!();
    }

    if !diff.changed.is_empty() {
        println!("{}", "Changed dependencies:".yellow().bold());
        for (name, old, new) in &diff.changed {
            println!("  ~ {}: {} → {}", name, old, new);
        }
        println!();
    }

    if !diff.unchanged.is_empty() {
        println!("{}", format!("Unchanged dependencies ({}):", diff.unchanged.len()).dimmed());
    }
}

pub fn print_what_if(result: &WhatIfResult, format: OutputFormat) {
    match format {
        OutputFormat::Text => print_what_if_text(result),
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct JsonWhatIf {
                success: bool,
                changes: Option<Vec<PackageChange>>,
            }
            match result {
                WhatIfResult::Success { changes, .. } => {
                    let json = JsonWhatIf { success: true, changes: Some(changes.clone()) };
                    println!("{}", serde_json::to_string_pretty(&json).unwrap());
                }
                _ => {
                    let json = JsonWhatIf { success: false, changes: None };
                    println!("{}", serde_json::to_string_pretty(&json).unwrap());
                }
            }
        }
        _ => {}
    }
}

fn print_what_if_text(result: &WhatIfResult) {
    match result {
        WhatIfResult::Success { changes, new_solution } => {
            println!("{}", "What-if analysis successful!".green().bold());
            println!();
            println!("Changes:");
            for change in changes {
                let symbol = match change.change_type {
                    ChangeType::Added => "+".green(),
                    ChangeType::Removed => "-".red(),
                    ChangeType::Upgraded => "↑".yellow(),
                    ChangeType::Downgraded => "↓".yellow(),
                };
                let old = change.old_version.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
                let new = change.new_version.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
                println!("  {} {}: {} → {}", symbol, change.package, old, new);
            }
            println!();
            println!("Total packages in new solution: {}", new_solution.versions.len());
        }
        WhatIfResult::Conflict { conflict } => {
            println!("{}", "What-if analysis resulted in a conflict:".red().bold());
            println!();
            print_conflict_text(conflict);
        }
        WhatIfResult::NoChange => {
            println!("{}", "No changes detected.".dimmed());
        }
    }
}

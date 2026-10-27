//! `cargo xtask layers`: the crate layering check (spec §14.2).
//!
//! Every workspace crate has a layer in [`LAYERS`], and an edge between
//! workspace crates must point to a strictly lower layer. The runtime crates
//! are isolated: `mora-rt-core` depends on no workspace crate and `mora-rt`
//! only on `mora-rt-core`. Any crate may take `mora-testkit` as a
//! dev-dependency, and nothing may depend on `xtask`, whose own edges are not
//! restricted.

use std::fmt;

use cargo_metadata::{DependencyKind, Metadata};

/// The test kit. Dev-dependency edges to it are always allowed.
pub const TESTKIT: &str = "mora-testkit";

/// The repository tool. It sits outside the layers.
pub const XTASK: &str = "xtask";

/// Each workspace crate and its layer, from §14.2. The runtime crates are in
/// layer 0, below the compiler, and have the extra rules of [`RUNTIME_DEPS`].
pub const LAYERS: &[(&str, u8)] = &[
    ("mora-rt-core", 0),
    ("mora-rt", 0),
    ("mora-span", 1),
    ("mora-diag", 1),
    ("mora-target", 1),
    ("mora-syntax", 2),
    ("mora-hir", 3),
    ("mora-check", 4),
    ("mora-mir", 4),
    ("mora-codegen", 5),
    ("mora-fmt", 5),
    ("mora-macro", 6),
    ("mora-link", 6),
    ("mora-pkg", 6),
    ("mora-driver", 6),
    ("mora-lsp", 7),
    ("mora-repl", 7),
    ("mora-cli", 8),
    ("mora-testkit", 8),
];

/// The only workspace crates each runtime crate may depend on.
pub const RUNTIME_DEPS: &[(&str, &[&str])] =
    &[("mora-rt-core", &[]), ("mora-rt", &["mora-rt-core"])];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A workspace crate that [`LAYERS`] does not list.
    UnknownCrate,
    /// An edge to a crate in the same or a higher layer.
    NotLower { from_layer: u8, to_layer: u8 },
    /// A runtime crate depending on a workspace crate outside [`RUNTIME_DEPS`].
    RuntimeIsolation,
    /// An edge to `xtask`.
    OnXtask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub from: String,
    pub to: String,
    pub kind: &'static str,
    pub problem: Problem,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.problem {
            Problem::UnknownCrate => write!(
                f,
                "{}: crate has no layer in xtask/src/layers.rs",
                self.from
            ),
            Problem::NotLower {
                from_layer,
                to_layer,
            } => write!(
                f,
                "{} (layer {from_layer}) -> {} (layer {to_layer}) ({}): a crate may only depend on lower layers",
                self.from, self.to, self.kind
            ),
            Problem::RuntimeIsolation => write!(
                f,
                "{} -> {} ({}): runtime crates may not depend on this crate",
                self.from, self.to, self.kind
            ),
            Problem::OnXtask => write!(
                f,
                "{} -> {} ({}): nothing may depend on xtask",
                self.from, self.to, self.kind
            ),
        }
    }
}

pub fn layer(name: &str) -> Option<u8> {
    LAYERS
        .iter()
        .find(|(crate_name, _)| *crate_name == name)
        .map(|(_, layer)| *layer)
}

fn kind_name(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Development => "dev",
        DependencyKind::Build => "build",
        _ => "normal",
    }
}

/// Checks every edge between workspace crates and returns the violations found.
pub fn check(metadata: &Metadata) -> Vec<Violation> {
    let packages = metadata.workspace_packages();
    let is_member = |name: &str| packages.iter().any(|p| p.name.as_str() == name);
    let mut violations = Vec::new();

    for package in &packages {
        let from = package.name.as_str();
        if from == XTASK {
            continue;
        }
        let Some(from_layer) = layer(from) else {
            violations.push(Violation {
                from: from.to_owned(),
                to: String::new(),
                kind: "normal",
                problem: Problem::UnknownCrate,
            });
            continue;
        };
        let runtime_deps = RUNTIME_DEPS
            .iter()
            .find(|(name, _)| *name == from)
            .map(|(_, deps)| *deps);

        for dep in &package.dependencies {
            let to = dep.name.as_str();
            if dep.path.is_none() || !is_member(to) {
                continue;
            }
            let violation = |problem| Violation {
                from: from.to_owned(),
                to: to.to_owned(),
                kind: kind_name(dep.kind),
                problem,
            };
            if to == XTASK {
                violations.push(violation(Problem::OnXtask));
                continue;
            }
            if dep.kind == DependencyKind::Development && to == TESTKIT {
                continue;
            }
            if let Some(allowed) = runtime_deps {
                if !allowed.contains(&to) {
                    violations.push(violation(Problem::RuntimeIsolation));
                }
                continue;
            }
            // An unknown target crate is reported once, as its own violation.
            let Some(to_layer) = layer(to) else { continue };
            if to_layer >= from_layer {
                violations.push(violation(Problem::NotLower {
                    from_layer,
                    to_layer,
                }));
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use serde_json::{Value, json};

    use super::*;

    /// `cargo metadata --no-deps` for this workspace, as JSON, fetched once.
    fn workspace_json() -> Value {
        static JSON: OnceLock<Value> = OnceLock::new();
        JSON.get_or_init(|| {
            let output = crate::metadata_command()
                .cargo_command()
                .output()
                .expect("run cargo metadata");
            assert!(output.status.success(), "cargo metadata failed");
            serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
        })
        .clone()
    }

    fn package<'a>(meta: &'a mut Value, name: &str) -> &'a mut Value {
        meta["packages"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("no package {name}"))
    }

    /// Adds a path dependency `from -> to` of the given kind to the metadata.
    fn add_dep(meta: &mut Value, from: &str, to: &str, kind: Option<&str>) {
        let dir = if to == XTASK {
            to.to_owned()
        } else {
            format!("crates/{to}")
        };
        let path = format!("{}/{dir}", meta["workspace_root"].as_str().unwrap());
        let dep = json!({
            "name": to,
            "source": null,
            "req": "^0.0.0",
            "kind": kind,
            "rename": null,
            "optional": false,
            "uses_default_features": true,
            "features": [],
            "target": null,
            "registry": null,
            "path": path,
        });
        package(meta, from)["dependencies"]
            .as_array_mut()
            .unwrap()
            .push(dep);
    }

    fn run(meta: Value) -> Vec<Violation> {
        check(&serde_json::from_value(meta).expect("metadata shape"))
    }

    fn single(meta: Value) -> Violation {
        let violations = run(meta);
        assert_eq!(violations.len(), 1, "{violations:?}");
        violations.into_iter().next().unwrap()
    }

    #[test]
    fn real_workspace_passes() {
        assert_eq!(run(workspace_json()), vec![]);
    }

    #[test]
    fn every_workspace_crate_has_a_layer() {
        let meta = workspace_json();
        for package in meta["packages"].as_array().unwrap() {
            let name = package["name"].as_str().unwrap();
            assert!(name == XTASK || layer(name).is_some(), "{name}");
        }
    }

    #[test]
    fn upward_edge_fails_and_names_the_edge() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-syntax", "mora-hir", None);
        let v = single(meta);
        assert_eq!(
            v.problem,
            Problem::NotLower {
                from_layer: 2,
                to_layer: 3
            }
        );
        assert_eq!(
            v.to_string(),
            "mora-syntax (layer 2) -> mora-hir (layer 3) (normal): a crate may only depend on lower layers"
        );
    }

    #[test]
    fn same_layer_edge_fails() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-mir", "mora-check", None);
        assert_eq!(
            single(meta).problem,
            Problem::NotLower {
                from_layer: 4,
                to_layer: 4
            }
        );
    }

    #[test]
    fn downward_edge_passes() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-mir", "mora-hir", None);
        add_dep(&mut meta, "mora-cli", "mora-driver", Some("build"));
        assert_eq!(run(meta), vec![]);
    }

    #[test]
    fn upward_build_and_dev_edges_fail() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-span", "mora-syntax", Some("build"));
        add_dep(&mut meta, "mora-diag", "mora-hir", Some("dev"));
        let kinds: Vec<&str> = run(meta).iter().map(|v| v.kind).collect();
        assert_eq!(kinds, ["build", "dev"]);
    }

    #[test]
    fn dev_edge_to_testkit_passes_from_any_layer() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-span", TESTKIT, Some("dev"));
        add_dep(&mut meta, "mora-syntax", TESTKIT, Some("dev"));
        assert_eq!(run(meta), vec![]);
    }

    #[test]
    fn dev_edge_to_testkit_passes_from_runtime_crates() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-rt-core", TESTKIT, Some("dev"));
        add_dep(&mut meta, "mora-rt", TESTKIT, Some("dev"));
        assert_eq!(run(meta), vec![]);
    }

    #[test]
    fn normal_edge_from_runtime_to_testkit_fails() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-rt-core", TESTKIT, None);
        assert_eq!(single(meta).problem, Problem::RuntimeIsolation);
    }

    #[test]
    fn normal_edge_to_testkit_fails() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-syntax", TESTKIT, None);
        assert_eq!(
            single(meta).problem,
            Problem::NotLower {
                from_layer: 2,
                to_layer: 8
            }
        );
    }

    #[test]
    fn runtime_core_depends_on_nothing() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-rt-core", "mora-span", None);
        let v = single(meta);
        assert_eq!(v.problem, Problem::RuntimeIsolation);
        assert_eq!(
            v.to_string(),
            "mora-rt-core -> mora-span (normal): runtime crates may not depend on this crate"
        );
    }

    #[test]
    fn hosted_runtime_depends_only_on_runtime_core() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-rt", "mora-target", None);
        assert_eq!(single(meta).problem, Problem::RuntimeIsolation);
    }

    #[test]
    fn edge_on_xtask_fails() {
        let mut meta = workspace_json();
        add_dep(&mut meta, "mora-cli", XTASK, Some("dev"));
        assert_eq!(single(meta).problem, Problem::OnXtask);
    }

    #[test]
    fn unknown_crate_fails() {
        let mut meta = workspace_json();
        let mut extra = package(&mut meta, "mora-span").clone();
        let id = format!("{}-extra", extra["id"].as_str().unwrap());
        extra["name"] = json!("mora-extra");
        extra["id"] = json!(id);
        meta["packages"].as_array_mut().unwrap().push(extra);
        meta["workspace_members"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
        let v = single(meta);
        assert_eq!(v.problem, Problem::UnknownCrate);
        assert_eq!(
            v.to_string(),
            "mora-extra: crate has no layer in xtask/src/layers.rs"
        );
    }
}

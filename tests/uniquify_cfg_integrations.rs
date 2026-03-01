use std::collections::HashSet;

use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use sand::AnnotatedExpression;
use sand::analysis::cfg::construct_cfg;
use sand::ir_types::hhir::Program;
use sand::passes::uniquify::reserved::SeenMap;
use sand::passes::uniquify::reserved::UniquifyError;
use sand::passes::uniquify::reserved::assert_unique;
use sand::passes::uniquify::reserved::check_expr;

// --------------------------- Helper

/// Recursively traverses a CFG and checks that all variables are unique.
///
/// # Arguments
/// * `graph` - The CFG graph.
/// * `start` - The NodeIndex to start traversing from.
/// * `seen` - Map of variable names to their first occurrence span.
/// * `visited` - Set of visited nodes to avoid infinite loops.
///
/// # Returns
/// Ok(()) if all variables are unique, otherwise Err(UniquifyError).
pub fn check_cfg_uniqueness(
    graph: &Graph<AnnotatedExpression, (), Directed>,
    start: NodeIndex,
    seen: &mut SeenMap,
    visited: &mut HashSet<NodeIndex>,
) -> Result<(), UniquifyError> {
    if visited.contains(&start) {
        return Ok(());
    }
    visited.insert(start);

    let node = &graph[start];

    check_expr(&node.expr, seen)?;

    for edge in graph.edges(start) {
        let target = edge.target();
        check_cfg_uniqueness(graph, target, seen, visited)?;
    }

    Ok(())
}

// ----------------------------- Helper

#[test]
fn simple() {
    let src = r#"
    def main(): Int := {
        let a:Int = 1;
        let a:Int = 1;
        let a:Int = a + a;
    }
    "#;

    let program = Program::parse(src).unwrap();
    let uniquified = program.uniquify().unwrap();
    assert!(assert_unique(&uniquified).is_ok());

    let cfg = construct_cfg(&uniquified).unwrap();
    check_cfg_uniqueness(
        &cfg,
        NodeIndex::new(0),
        &mut SeenMap::new(),
        &mut HashSet::new(),
    )
    .expect("cfg should be unique");
}

#[test]
fn if_statement() {
    let src = r#"
    def main(): Int := {
        let a:Int = 1;
        if a then {
            let a:Int = 1;
            let a:Int = a + a;
        };
        a
    }
    "#;

    let program = Program::parse(src).unwrap();
    let uniquified = program.uniquify().unwrap();
    assert!(assert_unique(&uniquified).is_ok());

    let cfg = construct_cfg(&uniquified).unwrap();
    check_cfg_uniqueness(
        &cfg,
        NodeIndex::new(0),
        &mut SeenMap::new(),
        &mut HashSet::new(),
    )
    .expect("cfg should be unique");
}

#[test]
fn while_loop() {
    let src = r#"
    def main(): Int := {
        let a:Int = 1;
        while a do {
            let a:Int = 1;
            let a:Int = a + a;
        };
        a
    }
    "#;

    let program = Program::parse(src).unwrap();
    let uniquified = program.uniquify().unwrap();
    assert!(assert_unique(&uniquified).is_ok());

    let cfg = construct_cfg(&uniquified).unwrap();
    check_cfg_uniqueness(
        &cfg,
        NodeIndex::new(0),
        &mut SeenMap::new(),
        &mut HashSet::new(),
    )
    .expect("cfg should be unique");
}

#[test]
fn functions() {
    let src = r#"
    def fn(a:Int):Int := {
        a
    }

    def main(): Int := {
        let a:Int = 1;
        let a:Int = fn(a);
        a
    }
    "#;

    let program = Program::parse(src).unwrap();
    let uniquified = program.uniquify().unwrap();
    assert!(assert_unique(&uniquified).is_ok());

    let cfg = construct_cfg(&uniquified).unwrap();
    check_cfg_uniqueness(
        &cfg,
        NodeIndex::new(0),
        &mut SeenMap::new(),
        &mut HashSet::new(),
    )
    .expect("cfg should be unique");
}

#[test]
fn complex() {
    let src = r#"
    def foo():Bool := {
        let a:Int = 1;
        let a:Bool = true;
        a & a
    }

    def bar(a:Bool):Bool := {
        if a then {
            while a do {
                let a: Bool = !a;
            };
        };
    }

    def main(): Int := {
        let a:Int = 1;
        if a then {
            let a:Int = 1;
            let x:Bool = foo();
            x = bar();
            let a:Int = a + a;
        };
        a
    }
    "#;

    let program = Program::parse(src).unwrap();
    let uniquified = program.uniquify().unwrap();
    assert!(assert_unique(&uniquified).is_ok());

    let cfg = construct_cfg(&uniquified).unwrap();
    check_cfg_uniqueness(
        &cfg,
        NodeIndex::new(0),
        &mut SeenMap::new(),
        &mut HashSet::new(),
    )
    .expect("cfg should be unique");
}

#[test]
fn blocks() {
    let src = r#"
    def main(): Int := {
        let a:Int = 1;
        let a:Int = {
            let a:Int = 1;
            let a:Int = a + a;
        };
        a
    }
    "#;

    let program = Program::parse(src).unwrap();
    let uniquified = program.uniquify().unwrap();
    assert!(assert_unique(&uniquified).is_ok());

    let cfg = construct_cfg(&uniquified).unwrap();
    check_cfg_uniqueness(
        &cfg,
        NodeIndex::new(0),
        &mut SeenMap::new(),
        &mut HashSet::new(),
    )
    .expect("cfg should be unique");
}

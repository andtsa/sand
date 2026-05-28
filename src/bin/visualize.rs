use std::fs;
use std::path::PathBuf;

use petgraph::dot::Config;
use petgraph::dot::Dot;
use sand::analysis::cfg;
use sand::castles::project::Project;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --bin visualize <source_file(s)>...");
        std::process::exit(1);
    }

    let proj = Project::from_paths(
        &args[1..]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    )?
    .ok();
    let (ctx, ast) = proj.check().result()?;
    let cfg = cfg::construct_cfg(&ctx, &ast);

    let dot = format!(
        "{:?}",
        Dot::with_attr_getters(
            &cfg,
            &[Config::EdgeNoLabel],
            &|_, _| String::new(),
            &|_, (_, node)| format!("label=\"{}\"", node)
        )
    );

    fs::write("cfg.dot", &dot)?;
    println!("CFG saved to cfg.dot");
    println!("Visualize with: dot -Tpng cfg.dot -o cfg.png");

    Ok(())
}

use std::fs;

use petgraph::dot::Config;
use petgraph::dot::Dot;
use sand::analysis::cfg;
use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --bin visualize <source_file>");
        std::process::exit(1);
    }

    let src = fs::read_to_string(&args[1])?;
    let mut ctx = CompileCtx::initial();
    let fr = ctx.register_dummy_file();
    let ast = compile_hir(Map::from([(fr, src.as_str())]), &mut ctx)?;
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

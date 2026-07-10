//! Generates one `#[test]` per annotated example in `../examples`.
//!
//! Annotation grammar
//!
//! ```text
//! //@TEST: run            // (or bare `//@TEST`, or `//@TEST: fail`)
//! // exit: 5              // run:  expected process exit code
//! // stdout: [1, 2, 3]    // run:  expected stdout lines
//! ```
//! ```text
//! //@TEST: fail
//! // stage: ownership     // fail: which stage rejected it
//!                         //       (parse|qualify|typecheck|ownership|internal)
//! // message: includes "moved inside a loop"   // fail: substring of the error
//! ```
//!
//! The directive lines are the contiguous `// key: value` comments immediately
//! after the `//@TEST` line. A bare `//@TEST` with no directives just asserts
//! the program compiles and runs. The (legacy) one-liner syntax
//! `//@TEST:{exit}[a,b]` is still accepted as well

use std::fs;
use std::path::Path;

enum Expect {
    Run {
        exit: Option<i32>,
        stdout: Option<Vec<String>>,
    },
    Fail {
        stage: Option<String>,
        message: Option<String>,
    },
}

/// Parse `[a, b, c]` into `["a", "b", "c"]` (empty list → `[]`).
fn parse_list(s: &str) -> Vec<String> {
    s.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a `message:` value: `includes "X"` → `X`; bare → trimmed/unquoted.
fn parse_message(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("includes").map(str::trim).unwrap_or(s);
    s.trim().trim_matches('"').to_string()
}

fn parse_annotation(content: &str) -> Option<Expect> {
    let lines: Vec<&str> = content.lines().collect();
    let idx = lines.iter().position(|l| {
        let t = l.trim();
        t == "//@TEST" || t.starts_with("//@TEST:") || t.starts_with("//@TEST ")
    })?;
    let head = lines[idx].trim();
    let mode = head
        .trim_start_matches("//@TEST")
        .trim_start_matches(':')
        .trim();
    let is_fail = mode == "fail";

    // Collect the contiguous `// key: value` directive lines after `//@TEST`.
    let mut directives: Vec<(String, String)> = Vec::new();
    for l in &lines[idx + 1..] {
        let Some(rest) = l.trim().strip_prefix("//") else {
            break; // a non-comment line ends the directive block
        };
        let rest = rest.trim();
        let Some((k, v)) = rest.split_once(':') else {
            break; // a `//` line that isn't `key: value` ends the block
        };
        let key = k.trim().to_lowercase();
        if !["exit", "stdout", "stage", "message"].contains(&key.as_str()) {
            break; // unknown key -> treat as prose, end the block
        }
        directives.push((key, v.trim().to_string()));
    }

    let get = |k: &str| {
        directives
            .iter()
            .find(|(dk, _)| dk == k)
            .map(|(_, v)| v.clone())
    };

    Some(if is_fail {
        Expect::Fail {
            stage: get("stage"),
            message: get("message").map(|v| parse_message(&v)),
        }
    } else {
        Expect::Run {
            exit: get("exit").and_then(|v| v.trim().parse().ok()),
            stdout: get("stdout").map(|v| parse_list(&v)),
        }
    })
}

fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

fn render_run(stem: &str, fn_name: &str, exit: Option<i32>, stdout: Option<Vec<String>>) -> String {
    let mut body = format!(
        "    let (code, output) = run_compiled(&[{stem:?}]).unwrap();\n    let _ = (code, &output);\n"
    );
    if let Some(e) = exit {
        body.push_str(&format!(
            "    assert_eq!(code, {e}, \"wrong exit code for {stem}\");\n"
        ));
    }
    if let Some(out) = stdout {
        let tokens = if out.is_empty() {
            "Vec::<String>::new()".to_string()
        } else {
            format!(
                "vec![{}]",
                out.iter()
                    .map(|s| format!("{s:?}.to_string()"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        body.push_str(&format!(
            "    assert_eq!(output, {tokens}, \"wrong stdout for {stem}\");\n"
        ));
    }
    format!("#[test]\nfn example_{fn_name}() {{\n{body}}}\n\n")
}

fn render_fail(
    stem: &str,
    fn_name: &str,
    stage: Option<String>,
    message: Option<String>,
) -> String {
    // NOTE: the expected `stage`/`message` are embedded only as the *operands*
    // (`{s:?}` / `{m:?}` produce quoted Rust literals); they must NOT also go in
    // the panic-message string, or their quotes/backticks would break the
    // generated literal. `assert_eq!`/`assert!` print the actual value anyway.
    let mut body = format!("    let f = compile_failure({stem:?});\n    let _ = &f;\n");
    if let Some(s) = stage {
        body.push_str(&format!(
            "    assert_eq!(f.stage, {s:?}, \"{stem}: failed at the wrong stage\");\n"
        ));
    }
    if let Some(m) = message {
        body.push_str(&format!(
            "    assert!(f.message.contains({m:?}), \"{stem}: unexpected error: {{:?}}\", f.message);\n"
        ));
    }
    format!("#[test]\nfn example_{fn_name}() {{\n{body}}}\n\n")
}

/// Recursively collect every `.sand` path under `dir`.
fn collect_sand(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("examples dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_sand(&path, out);
        } else if path.extension().is_some_and(|e| e == "sand") {
            out.push(path);
        }
    }
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let examples_dir = Path::new(&manifest_dir).parent().unwrap().join("examples");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("compiled_examples_tests.rs");

    println!("cargo:rerun-if-changed={}", examples_dir.display());

    let mut entries = Vec::new();
    collect_sand(&examples_dir, &mut entries);

    let mut tests = String::new();
    for path in &entries {
        println!("cargo:rerun-if-changed={}", path.display());
        let content = fs::read_to_string(path).expect("read example");
        let Some(expect) = parse_annotation(&content) else {
            continue;
        };
        // Name passed to the runtime helpers is the path relative to `examples/`
        // (minus the `.sand` extension), e.g. `ownership/borrowing`. Both
        // `run_compiled` and `compile_failure` resolve `examples/{name}.sand`,
        // so the subfolder is part of the name. Normalise separators to `/`.
        let rel = path.strip_prefix(&examples_dir).unwrap().with_extension("");
        let name = rel
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let fn_name = sanitize_ident(&name);
        tests.push_str(&match expect {
            Expect::Run { exit, stdout } => render_run(&name, &fn_name, exit, stdout),
            Expect::Fail { stage, message } => render_fail(&name, &fn_name, stage, message),
        });
    }

    fs::write(&out_path, tests).expect("write generated tests");
}

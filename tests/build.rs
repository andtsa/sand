use std::fs;
use std::path::Path;

fn parse_test_annotation(content: &str) -> Option<(i32, Vec<String>)> {
    let test_line = content
        .lines()
        .find(|l| l.starts_with("//@TEST:{"))?
        .trim_start_matches("//@TEST:{")
        .trim_end_matches('}');
    let (code, output) = test_line.split_once("}[")?;
    let exit_code = code.parse::<i32>().ok()?;
    let expected_output = output
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Some((exit_code, expected_output))
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

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let examples_dir = Path::new(&manifest_dir).parent().unwrap().join("examples");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("compiled_examples_tests.rs");

    println!("cargo:rerun-if-changed={}", examples_dir.display());

    let mut tests = String::new();

    let mut entries: Vec<_> = fs::read_dir(&examples_dir)
        .expect("examples dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sand"))
        .collect();
    entries.sort();

    for path in &entries {
        println!("cargo:rerun-if-changed={}", path.display());
        let content = fs::read_to_string(path).expect("read example");
        let Some((expected_code, expected_output)) = parse_test_annotation(&content) else {
            continue;
        };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let fn_name = sanitize_ident(stem);
        let output_tokens = if expected_output.is_empty() {
            "Vec::<String>::new()".to_string()
        } else {
            format!(
                "vec![{}]",
                expected_output
                    .iter()
                    .map(|s| format!("{:?}.to_string()", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        tests.push_str(&format!(
            r#"#[test]
fn example_{fn_name}() {{
    let (code, output) = run_compiled(&[{stem:?}]).unwrap();
    assert_eq!(code, {expected_code}, "wrong exit code for {stem}");
    assert_eq!(output, {output_tokens}, "wrong output for {stem}");
}}

"#
        ));
    }

    fs::write(&out_path, tests).expect("write generated tests");
}

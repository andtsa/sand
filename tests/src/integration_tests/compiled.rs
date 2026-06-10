//! run examples through the whole compiler

use std::path::Path;

use crate::common::project_root;
use crate::common::run_compiled;

/// look for the exepcted output from a leading
/// `//@TEST:{exit-code}[printed-line-1,printed-line-2]`
/// comment in the file.
pub fn test_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Option<(i32, Vec<String>)>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<_> = content.lines().collect();
    let Some(test_line) = lines
        .iter()
        .find(|l| l.starts_with("//@TEST:{"))
        .map(|l| l.trim_start_matches("//@TEST:{").trim_end_matches('}'))
    else {
        return Ok(None);
    };
    let (exit_code, expected_output) = test_line
        .split_once("}[")
        .and_then(|(code, output)| {
            Some((
                code.parse::<i32>().ok()?,
                output
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.to_string())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>(),
            ))
        })
        .ok_or_else(|| anyhow::anyhow!("invalid test line format"))?;
    Ok(Some((exit_code, expected_output)))
}

#[cfg(test)]
fn assert_return(input: &[&str], expected: i32) -> anyhow::Result<()> {
    let (result, _) = run_compiled(input)?;
    assert_eq!(result, expected);
    Ok(())
}

#[cfg(test)]
fn assert_output(input: &[&str], expected: &[&str]) -> anyhow::Result<()> {
    let (_, output_lines) = run_compiled(input)?;
    assert_eq!(output_lines, expected);
    Ok(())
}

#[test]
fn compiled_factorial() {
    assert_output(&["fact"], &["362880"]).unwrap();
}

#[test]
fn compiled_prime() {
    assert_return(&["prime"], 97).unwrap();
}

#[test]
fn compiled_gcd() {
    assert_return(&["gcd"], 1).unwrap();
}

#[test]
fn test_all_annotated_examples() -> anyhow::Result<()> {
    let example_dir = project_root().join("examples");
    let examples = std::fs::read_dir(&example_dir)?;
    for example in examples {
        let path = example?.path();
        if path.is_file() {
            let wrap_err =
                |e: anyhow::Error| anyhow::anyhow!("failed to test {}: {}", path.display(), e);
            if path.extension().is_none_or(|e| e != "sand") {
                continue;
            }
            let Some((expected_code, expected_output)) = test_from_file(&path).map_err(wrap_err)?
            else {
                continue;
            };
            let (actual_code, actual_output) =
                run_compiled(&[path.file_stem().and_then(|s| s.to_str()).unwrap()])
                    .map_err(wrap_err)?;
            assert_eq!(
                actual_code,
                expected_code,
                "while running {}: expected code {} but got {}",
                path.display(),
                expected_code,
                actual_code
            );
            assert_eq!(
                actual_output,
                expected_output,
                "while running {}: expected output {:?} but got {:?}",
                path.display(),
                expected_output,
                actual_output
            );
        }
    }
    Ok(())
}

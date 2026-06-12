//! analyse a single file
//!
//! - read input file from command line args
//! - find repeated expressions
//! - print to stdout
//!
//! TODO: move this to the main CLI

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use lang::analysis::ProgramAnnotations;
use lang::analysis::analyse;
use lang::analysis::flipped_occurence_map;
use lang::analysis::interactions::has_other_side_effects;
use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::FileRef;
use lang::compiler::structure::Range;
use lang::ir_types::typed_hir::Expr;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input_file(s)>...", args[0]);
        std::process::exit(1);
    }

    let proj = Project::from_paths(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>())?.ok();
    let (ctx, ast) = proj.check().result_leaked()?;
    let annotations = analyse(&ctx, &ast);

    println!(
        "Program Annotations:\n{}",
        visualise_annotations(&ctx, &proj, annotations)
    );

    Ok(())
}

type OccurenceMap<'a, 'tcx> = HashMap<&'a Expr<'tcx>, HashSet<Range>>;

fn visualise_annotations<'tcx>(
    ctx: &CompileCtx<'tcx>,
    proj: &Project,
    annotations: ProgramAnnotations<'tcx>,
) -> String {
    let mut files: HashMap<FileRef, OccurenceMap<'_, 'tcx>> = HashMap::new();
    for (mr, hm) in flipped_occurence_map(&annotations.expr_occurrences) {
        files
            .entry(ctx.file_of_module(mr))
            .and_modify(|h| {
                for (e, occs) in &hm {
                    h.entry(e)
                        .and_modify(|s: &mut HashSet<Range>| s.extend(occs.iter()))
                        .or_insert(occs.clone());
                }
            })
            .or_insert(hm);
    }

    let mut out = String::new();
    for (f, occs) in files {
        out.push_str(&format!("File {}:\n", proj.file_name(f)));

        out.push_str(&visualise_for_file(proj.text_for_file(f).unwrap(), &occs));

        out.push('\n');
    }

    out
}

// `Expr` keys reach an enum payload `Cell` through arena references but hash by
// structural/pointer identity that never reads it, so the keys are stable.
#[allow(clippy::mutable_key_type)]
fn visualise_for_file(text: &str, repeated_expressions: &OccurenceMap<'_, '_>) -> String {
    // collect lines preserving newline characters
    let lines_inclusive: Vec<&str> = text.split_inclusive('\n').collect();

    // precompute char counts (without the newline) for each line
    let mut line_char_counts: Vec<usize> = Vec::with_capacity(lines_inclusive.len());
    let mut line_stripped: Vec<String> = Vec::with_capacity(lines_inclusive.len());
    for l in &lines_inclusive {
        line_char_counts.push(l.trim_end().chars().count());
        line_stripped.push(l.trim_end().to_string());
    }

    // map from 1-based line number -> Vec<(start_col, end_col)> inclusive, both
    // 1-based
    let mut ranges_by_line: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();

    for (expr, occs) in repeated_expressions.iter() {
        if has_other_side_effects(expr) {
            continue;
        }
        for ((sl, sc), (el, ec)) in occs.iter().map(|r| r.destruct()) {
            if sl == 0 || el == 0 {
                continue;
            }
            let max_line = lines_inclusive.len();
            if sl > max_line || el > max_line {
                continue;
            }

            if sl == el {
                ranges_by_line.entry(sl).or_default().push((sc, ec));
            } else if sl < el {
                // start line: from sc to end
                let start_line_len = line_char_counts.get(sl - 1).copied().unwrap_or(0);
                if start_line_len > 0 && sc <= start_line_len + 1 {
                    ranges_by_line
                        .entry(sl)
                        .or_default()
                        .push((sc, start_line_len));
                }

                // middle full lines
                for ln in (sl + 1)..el {
                    let l_len = line_char_counts.get(ln - 1).copied().unwrap_or(0);
                    if l_len > 0 {
                        ranges_by_line.entry(ln).or_default().push((1, l_len));
                    }
                }

                // end line: from 1 to ec
                let end_line_len = line_char_counts.get(el - 1).copied().unwrap_or(0);
                if end_line_len > 0 && ec >= 1 {
                    let ec_clamped = if ec > end_line_len { end_line_len } else { ec };
                    if ec_clamped >= 1 {
                        ranges_by_line.entry(el).or_default().push((1, ec_clamped));
                    }
                }
            } else {
                // sl > el: malformed - ignore
                continue;
            }
        }
    }

    // merge ranges on each line (ranges are 1-based inclusive)
    for (_ln, ranges) in ranges_by_line.iter_mut() {
        if ranges.is_empty() {
            continue;
        }
        ranges.sort_by_key(|(s, _e)| *s);
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        let mut cur = ranges[0];
        for &(s, e) in &ranges[1..] {
            // if next.start is <= cur.end + 1 => merge (adjacent or overlapping)
            if s <= cur.1 + 1 {
                if e > cur.1 {
                    cur.1 = e;
                }
            } else {
                merged.push(cur);
                cur = (s, e);
            }
        }
        merged.push(cur);
        *ranges = merged;
    }

    // ANSI sequences
    let start_seq = "\x1b[1;33m"; // bold yellow
    let reset_seq = "\x1b[0m";

    // rebuild the text
    let mut out = String::with_capacity(text.len() * 2);
    for (idx, orig_line) in lines_inclusive.iter().enumerate() {
        let ln = idx + 1;
        let has_nl = orig_line.ends_with('\n');
        let line_content = &line_stripped[idx];
        let chars: Vec<char> = line_content.chars().collect();
        let len = chars.len();

        if let Some(ranges) = ranges_by_line.get(&ln) {
            let mut cur_pos = 0usize; // 0-based
            for &(s1, e1) in ranges.iter() {
                // convert to 0-based inclusive indices: start0 = s1-1, end_inclusive = min(e1,
                // len)
                if s1 == 0 || e1 == 0 {
                    continue;
                }
                let start0 = s1.saturating_sub(1);
                let end_inclusive = if e1 > len {
                    len
                } else {
                    e1 - 1 /* not completely sure why this is needed but it doesnt work without */
                };
                if start0 >= end_inclusive {
                    continue;
                }
                // append prefix (cur_pos .. start0)
                if cur_pos < start0 && cur_pos < len {
                    out.extend(chars[cur_pos..start0].iter());
                }
                // append highlight
                out.push_str(start_seq);
                out.extend(chars[start0..end_inclusive].iter());
                out.push_str(reset_seq);
                cur_pos = end_inclusive;
            }
            // append remainder
            if cur_pos < len {
                out.extend(chars[cur_pos..len].iter());
            }
        } else {
            // no highlights on this line; append as is
            out.push_str(line_content);
        }

        if has_nl {
            out.push('\n');
        }
    }

    out
}

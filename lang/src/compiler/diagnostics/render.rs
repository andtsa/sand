use crate::castles::project::Project;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Range;

impl SandDiagnostic {
    /// render the diagnostic in rustc-style, with source snippets
    ///
    /// ```text
    /// error: unbound variable 'foo'
    ///  --> main.sand:3:5
    ///   |
    /// 3 |     foo + 1
    ///   |     ^^^ no binding found for this variable
    /// ```
    pub fn render(&self, project: &Project) -> String {
        let mut out = format!("{}: {}", self.severity, self.message);

        if let Some(fr) = self.file {
            let file_name = file_display_name(project, fr);
            if let Some(source) = project.text_for_file(fr) {
                out.push('\n');
                out.push_str(&render_snippet(source, &file_name, self.range, ""));
            }
        }

        for info in &self.related {
            out.push('\n');
            out.push_str(&render_related(project, info));
        }

        out
    }
}

impl DiagnosticSeverity {
    pub fn render(&self, _ansi: bool) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::CompilerDebug => "compiler debug",
        }
    }
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(false))
    }
}

fn render_related(project: &Project, info: &SdRelatedInfo) -> String {
    let file_name = file_display_name(project, info.file);
    let mut out = format!("note: {}", info.message);
    if let Some(source) = project.text_for_file(info.file) {
        out.push('\n');
        out.push_str(&render_snippet(source, &file_name, info.range, ""));
    }
    out
}

/// renders a source snippet in rustc style:
/// ```text
///  --> file:line:col
///   |
/// 3 |     foo + 1
///   |     ^^^
/// ```
/// `annotation` is placed after the carets on the same line (may be empty)
fn render_snippet(source: &str, file_name: &str, range: Range, annotation: &str) -> String {
    let line_num = range.start.line; // 1-based
    let col = range.start.col; // 1-based

    let gutter = digits(line_num);
    // gutter spaces before `-->`, gutter+1 spaces before `|` lines
    let pad = " ".repeat(gutter);
    let gpad = " ".repeat(gutter + 1); // aligns `|` with the space after the line number

    // extract the relevant source line (0-based index)
    let source_line = source.lines().nth(line_num.saturating_sub(1)).unwrap_or("");

    // caret span: columns are 1-based; span from start to end on the same line
    let caret_start = col.saturating_sub(1); // 0-based offset into line
    let caret_end = if range.end.line == range.start.line {
        range.end.col.saturating_sub(1)
    } else {
        source_line.len()
    };
    let caret_len = (caret_end.saturating_sub(caret_start)).max(1);
    let leading = " ".repeat(caret_start);
    let carets = "^".repeat(caret_len);

    let annotation_part = if annotation.is_empty() {
        String::new()
    } else {
        format!(" {}", annotation)
    };

    format!(
        "{pad} --> {file_name}:{line_num}:{col}\n\
         {gpad}|\n\
         {line_num:>gutter$} | {source_line}\n\
         {gpad}| {leading}{carets}{annotation_part}",
    )
}

fn file_display_name(project: &Project, fr: FileRef) -> String {
    project
        .uri_of_file(fr)
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("?")
        .to_string()
}

fn digits(n: usize) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}

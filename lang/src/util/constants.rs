//! The code in this module is borrowed from [the `gourd` project](https://github.com/ConSol-Lab/gourd)
use anstyle::AnsiColor;
use anstyle::Color;
use anstyle::Style;

/// The styling for error messages.
pub const ERROR_STYLE: Style = style_from_fg(AnsiColor::Red).bold();

/// The styling for warning messages.
pub const WARNING_STYLE: Style = style_from_fg(AnsiColor::Yellow).bold();

/// The styling for help messages.
pub const HELP_STYLE: Style = style_from_fg(AnsiColor::Green).bold().underline();

/// Create a style with a defined foreground color.
pub const fn style_from_fg(color: AnsiColor) -> Style {
    Style::new().fg_color(Some(Color::Ansi(color)))
}

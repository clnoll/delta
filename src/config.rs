use std::cmp::min;
use std::path::PathBuf;
use std::process;

use console::Term;
use syntect::highlighting::Style as SyntectStyle;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::bat::output::PagingMode;
use crate::cli::{self, unreachable};
use crate::color;
use crate::delta::State;
use crate::env;
use crate::style::Style;
use crate::theme;

pub enum Width {
    Fixed(usize),
    Variable,
}

pub struct Config<'a> {
    pub background_color_extends_to_terminal_width: bool,
    pub commit_style: Style,
    pub decorations_width: Width,
    pub dummy_theme: Theme,
    pub file_added_label: String,
    pub file_modified_label: String,
    pub file_removed_label: String,
    pub file_renamed_label: String,
    pub file_style: Style,
    pub hunk_header_style: Style,
    pub max_buffered_lines: usize,
    pub max_line_distance: f64,
    pub max_line_distance_for_naively_paired_lines: f64,
    pub minus_emph_style: Style,
    pub minus_file: Option<PathBuf>,
    pub minus_line_marker: &'a str,
    pub minus_non_emph_style: Style,
    pub minus_style: Style,
    pub navigate: bool,
    pub null_style: Style,
    pub null_syntect_style: SyntectStyle,
    pub number_minus_format: String,
    pub number_minus_format_style: Style,
    pub number_minus_style: Style,
    pub number_plus_format: String,
    pub number_plus_format_style: Style,
    pub number_plus_style: Style,
    pub paging_mode: PagingMode,
    pub plus_emph_style: Style,
    pub plus_file: Option<PathBuf>,
    pub plus_line_marker: &'a str,
    pub plus_non_emph_style: Style,
    pub plus_style: Style,
    pub show_line_numbers: bool,
    pub syntax_set: SyntaxSet,
    pub tab_width: usize,
    pub theme: Option<Theme>,
    pub theme_name: String,
    pub true_color: bool,
    pub zero_style: Style,
}

impl<'a> Config<'a> {
    pub fn get_style(&self, state: &State) -> &Style {
        match state {
            State::CommitMeta => &self.commit_style,
            State::FileMeta => &self.file_style,
            State::HunkHeader => &self.hunk_header_style,
            _ => unreachable("Unreachable code reached in get_style."),
        }
    }

    pub fn make_navigate_regexp(&self) -> String {
        format!(
            "^(commit|{}|{}|{}|{})",
            self.file_modified_label,
            self.file_added_label,
            self.file_removed_label,
            self.file_renamed_label
        )
    }
}

pub fn get_config<'a>(
    opt: cli::Opt,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    true_color: bool,
    paging_mode: PagingMode,
) -> Config<'a> {
    // Allow one character for e.g. `less --status-column` is in effect. See #41 and #10.
    let available_terminal_width = (Term::stdout().size().1 - 1) as usize;
    let (decorations_width, background_color_extends_to_terminal_width) = match opt.width.as_deref()
    {
        Some("variable") => (Width::Variable, false),
        Some(width) => {
            let width = width.parse().unwrap_or_else(|_| {
                eprintln!("Could not parse width as a positive integer: {:?}", width);
                process::exit(1);
            });
            (Width::Fixed(min(width, available_terminal_width)), true)
        }
        None => (Width::Fixed(available_terminal_width), true),
    };

    let theme_name_from_bat_pager = env::get_env_var("BAT_THEME");
    let (is_light_mode, theme_name) = theme::get_is_light_mode_and_theme_name(
        opt.theme.as_ref(),
        theme_name_from_bat_pager.as_ref(),
        opt.light,
        &theme_set,
    );

    let (
        minus_style,
        minus_emph_style,
        minus_non_emph_style,
        zero_style,
        plus_style,
        plus_emph_style,
        plus_non_emph_style,
    ) = make_hunk_styles(&opt, is_light_mode, true_color);

    let (commit_style, file_style, hunk_header_style) =
        make_commit_file_hunk_header_styles(&opt, true_color);

    let (
        number_minus_format_style,
        number_minus_style,
        number_plus_format_style,
        number_plus_style,
    ) = make_line_number_styles(&opt, is_light_mode, true_color);

    let theme = if theme::is_no_syntax_highlighting_theme_name(&theme_name) {
        None
    } else {
        Some(theme_set.themes[&theme_name].clone())
    };
    let dummy_theme = theme_set.themes.values().next().unwrap().clone();

    let minus_line_marker = if opt.keep_plus_minus_markers {
        "-"
    } else {
        " "
    };
    let plus_line_marker = if opt.keep_plus_minus_markers {
        "+"
    } else {
        " "
    };

    let max_line_distance_for_naively_paired_lines =
        env::get_env_var("DELTA_EXPERIMENTAL_MAX_LINE_DISTANCE_FOR_NAIVELY_PAIRED_LINES")
            .map(|s| s.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

    Config {
        background_color_extends_to_terminal_width,
        commit_style,
        decorations_width,
        dummy_theme,
        file_added_label: opt.file_added_label,
        file_modified_label: opt.file_modified_label,
        file_removed_label: opt.file_removed_label,
        file_renamed_label: opt.file_renamed_label,
        file_style,
        hunk_header_style,
        max_buffered_lines: 32,
        max_line_distance: opt.max_line_distance,
        max_line_distance_for_naively_paired_lines,
        minus_emph_style,
        minus_file: opt.minus_file.map(|s| s.clone()),
        minus_line_marker,
        minus_non_emph_style,
        minus_style,
        navigate: opt.navigate,
        null_style: Style::new(),
        null_syntect_style: SyntectStyle::default(),
        number_minus_format: opt.number_minus_format,
        number_minus_format_style: number_minus_format_style,
        number_minus_style: number_minus_style,
        number_plus_format: opt.number_plus_format,
        number_plus_format_style: number_plus_format_style,
        number_plus_style: number_plus_style,
        paging_mode,
        plus_emph_style,
        plus_file: opt.plus_file.map(|s| s.clone()),
        plus_line_marker,
        plus_non_emph_style,
        plus_style,
        show_line_numbers: opt.show_line_numbers,
        syntax_set,
        tab_width: opt.tab_width,
        theme,
        theme_name,
        true_color,
        zero_style,
    }
}

fn make_hunk_styles<'a>(
    opt: &'a cli::Opt,
    is_light_mode: bool,
    true_color: bool,
) -> (Style, Style, Style, Style, Style, Style, Style) {
    let minus_style = Style::from_str(
        &opt.minus_style,
        None,
        Some(color::get_minus_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        false,
    );

    let minus_emph_style = Style::from_str(
        &opt.minus_emph_style,
        None,
        Some(color::get_minus_emph_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        true,
    );

    let minus_non_emph_style = Style::from_str(
        &opt.minus_non_emph_style,
        minus_style.ansi_term_style.foreground,
        minus_style.ansi_term_style.background,
        None,
        true_color,
        false,
    );

    let zero_style = Style::from_str(&opt.zero_style, None, None, None, true_color, false);

    let plus_style = Style::from_str(
        &opt.plus_style,
        None,
        Some(color::get_plus_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        false,
    );

    let plus_emph_style = Style::from_str(
        &opt.plus_emph_style,
        None,
        Some(color::get_plus_emph_background_color_default(
            is_light_mode,
            true_color,
        )),
        None,
        true_color,
        true,
    );

    let plus_non_emph_style = Style::from_str(
        &opt.plus_non_emph_style,
        plus_style.ansi_term_style.foreground,
        plus_style.ansi_term_style.background,
        None,
        true_color,
        false,
    );

    (
        minus_style,
        minus_emph_style,
        minus_non_emph_style,
        zero_style,
        plus_style,
        plus_emph_style,
        plus_non_emph_style,
    )
}

fn opt_or_default<'a>(option: &'a str, default: &'a str) -> &'a str {
    match option == "".to_string() {
        true => default,
        false => option,
    }
}

fn make_line_number_styles<'a>(
    opt: &'a cli::Opt,
    is_light_mode: bool,
    true_color: bool,
) -> (Style, Style, Style, Style) {
    let number_minus_format_style = Style::from_str(
        opt_or_default(&opt.number_minus_format_style, &opt.hunk_header_style),
        None,
        None,
        None,
        true_color,
        false,
    );

    let number_minus_style = Style::from_str(
        opt_or_default(&opt.number_minus_style, &opt.hunk_header_style),
        None,
        None,
        None,
        true_color,
        false,
    );

    let number_plus_format_style = Style::from_str(
        opt_or_default(&opt.number_plus_format_style, &opt.hunk_header_style),
        None,
        None,
        None,
        true_color,
        false,
    );

    let number_plus_style = Style::from_str(
        opt_or_default(&opt.number_plus_style, &opt.hunk_header_style),
        None,
        None,
        None,
        true_color,
        false,
    );

    (
        number_minus_format_style,
        number_minus_style,
        number_plus_format_style,
        number_plus_style,
    )
}

fn make_commit_file_hunk_header_styles(opt: &cli::Opt, true_color: bool) -> (Style, Style, Style) {
    (
        Style::from_str_with_handling_of_special_decoration_attributes_and_respecting_deprecated_foreground_color_arg(
            &opt.commit_style,
            None,
            None,
            Some(&opt.commit_decoration_style),
            opt.deprecated_commit_color.as_deref(),
            true_color,
            false,
        ),
        Style::from_str_with_handling_of_special_decoration_attributes_and_respecting_deprecated_foreground_color_arg(
            &opt.file_style,
            None,
            None,
            Some(&opt.file_decoration_style),
            opt.deprecated_file_color.as_deref(),
            true_color,
            false,
        ),
        Style::from_str_with_handling_of_special_decoration_attributes_and_respecting_deprecated_foreground_color_arg(
            &opt.hunk_header_style,
            None,
            None,
            Some(&opt.hunk_header_decoration_style),
            opt.deprecated_hunk_color.as_deref(),
            true_color,
            false,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    use crate::cli;
    use crate::color;
    use crate::tests::integration_test_utils::integration_test_utils;

    #[test]
    #[ignore]
    fn test_theme_selection() {
        #[derive(PartialEq)]
        enum Mode {
            Light,
            Dark,
        };
        for (
            theme_option,
            bat_theme_env_var,
            mode_option, // (--light, --dark)
            expected_theme,
            expected_mode,
        ) in vec![
            (None, "", None, theme::DEFAULT_DARK_THEME, Mode::Dark),
            (Some("GitHub".to_string()), "", None, "GitHub", Mode::Light),
            (
                Some("GitHub".to_string()),
                "1337",
                None,
                "GitHub",
                Mode::Light,
            ),
            (None, "1337", None, "1337", Mode::Dark),
            (
                None,
                "<not set>",
                None,
                theme::DEFAULT_DARK_THEME,
                Mode::Dark,
            ),
            (
                None,
                "",
                Some(Mode::Light),
                theme::DEFAULT_LIGHT_THEME,
                Mode::Light,
            ),
            (
                None,
                "",
                Some(Mode::Dark),
                theme::DEFAULT_DARK_THEME,
                Mode::Dark,
            ),
            (
                None,
                "<@@@@@>",
                Some(Mode::Light),
                theme::DEFAULT_LIGHT_THEME,
                Mode::Light,
            ),
            (None, "1337", Some(Mode::Light), "1337", Mode::Light),
            (Some("none".to_string()), "", None, "none", Mode::Dark),
            (
                Some("None".to_string()),
                "",
                Some(Mode::Light),
                "None",
                Mode::Light,
            ),
        ] {
            if bat_theme_env_var == "<not set>" {
                env::remove_var("BAT_THEME")
            } else {
                env::set_var("BAT_THEME", bat_theme_env_var);
            }
            let is_true_color = true;
            let mut options = integration_test_utils::get_command_line_options();
            options.theme = theme_option;
            match mode_option {
                Some(Mode::Light) => {
                    options.light = true;
                    options.dark = false;
                }
                Some(Mode::Dark) => {
                    options.light = false;
                    options.dark = true;
                }
                None => {
                    options.light = false;
                    options.dark = false;
                }
            }
            let config = cli::process_command_line_arguments(options, None);
            assert_eq!(config.theme_name, expected_theme);
            if theme::is_no_syntax_highlighting_theme_name(expected_theme) {
                assert!(config.theme.is_none())
            } else {
                assert_eq!(config.theme.unwrap().name.as_ref().unwrap(), expected_theme);
            }
            assert_eq!(
                config.minus_style.ansi_term_style.background.unwrap(),
                color::get_minus_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
            assert_eq!(
                config.minus_emph_style.ansi_term_style.background.unwrap(),
                color::get_minus_emph_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
            assert_eq!(
                config.plus_style.ansi_term_style.background.unwrap(),
                color::get_plus_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
            assert_eq!(
                config.plus_emph_style.ansi_term_style.background.unwrap(),
                color::get_plus_emph_background_color_default(
                    expected_mode == Mode::Light,
                    is_true_color
                )
            );
        }
    }
}

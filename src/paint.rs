use lazy_static::lazy_static;
use regex::Regex;
use std::io::Write;

use ansi_term;
use syntect::easy::HighlightLines;
use syntect::highlighting::Style as SyntectStyle;
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::config;
use crate::delta::State;
use crate::edits;
use crate::paint::superimpose_style_sections::superimpose_style_sections;
use crate::style::Style;

pub const ANSI_CSI_ERASE_IN_LINE: &str = "\x1b[K";
pub const ANSI_SGR_RESET: &str = "\x1b[0m";

pub struct Painter<'a> {
    pub minus_lines: Vec<String>,
    pub plus_lines: Vec<String>,
    pub writer: &'a mut dyn Write,
    pub syntax: &'a SyntaxReference,
    pub highlighter: HighlightLines<'a>,
    pub config: &'a config::Config<'a>,
    pub output_buffer: String,
    pub minus_line_number: usize,
    pub plus_line_number: usize,
}

impl<'a> Painter<'a> {
    pub fn new(writer: &'a mut dyn Write, config: &'a config::Config) -> Self {
        let default_syntax = Self::get_syntax(&config.syntax_set, None);
        // TODO: Avoid doing this.
        let dummy_highlighter = HighlightLines::new(default_syntax, &config.dummy_theme);
        Self {
            minus_lines: Vec::new(),
            plus_lines: Vec::new(),
            output_buffer: String::new(),
            syntax: default_syntax,
            highlighter: dummy_highlighter,
            writer,
            config,
            minus_line_number: 0,
            plus_line_number: 0,
        }
    }

    pub fn set_syntax(&mut self, extension: Option<&str>) {
        self.syntax = Painter::get_syntax(&self.config.syntax_set, extension);
    }

    fn get_syntax(syntax_set: &'a SyntaxSet, extension: Option<&str>) -> &'a SyntaxReference {
        syntax_set
            .find_syntax_by_extension(extension.unwrap_or("txt"))
            .unwrap_or_else(|| Painter::get_syntax(syntax_set, Some("txt")))
    }

    pub fn set_highlighter(&mut self) {
        if let Some(ref theme) = self.config.theme {
            self.highlighter = HighlightLines::new(self.syntax, &theme)
        };
    }

    pub fn paint_buffered_lines(&mut self) {
        let minus_line_syntax_style_sections = Self::get_syntax_style_sections_for_lines(
            &self.minus_lines,
            &State::HunkMinus,
            &mut self.highlighter,
            self.config,
        );
        let plus_line_syntax_style_sections = Self::get_syntax_style_sections_for_lines(
            &self.plus_lines,
            &State::HunkPlus,
            &mut self.highlighter,
            self.config,
        );
        let (minus_line_diff_style_sections, plus_line_diff_style_sections) =
            Self::get_diff_style_sections(&self.minus_lines, &self.plus_lines, self.config);

        let mut minus_line_numbers = Vec::new();
        let mut plus_line_numbers = Vec::new();
        for _line in &self.minus_lines {
            minus_line_numbers.push((Some(self.minus_line_number), None));
            self.minus_line_number += 1;
        }
        for _line in &self.plus_lines {
            plus_line_numbers.push((None, Some(self.plus_line_number)));
            self.plus_line_number += 1;
        }
        // TODO: lines and style sections contain identical line text
        if !self.minus_lines.is_empty() {
            Painter::paint_lines(
                minus_line_syntax_style_sections,
                minus_line_diff_style_sections,
                minus_line_numbers,
                &mut self.output_buffer,
                self.config,
                self.config.minus_line_marker,
                self.config.minus_style,
                self.config.minus_non_emph_style,
                None,
            );
        }
        if !self.plus_lines.is_empty() {
            Painter::paint_lines(
                plus_line_syntax_style_sections,
                plus_line_diff_style_sections,
                plus_line_numbers,
                &mut self.output_buffer,
                self.config,
                self.config.plus_line_marker,
                self.config.plus_style,
                self.config.plus_non_emph_style,
                None,
            );
        }
        self.minus_lines.clear();
        self.plus_lines.clear();
    }

    /// Superimpose background styles and foreground syntax
    /// highlighting styles, and write colored lines to output buffer.
    pub fn paint_lines(
        syntax_style_sections: Vec<Vec<(SyntectStyle, &str)>>,
        diff_style_sections: Vec<Vec<(Style, &str)>>,
        line_number_sections: Vec<(Option<usize>, Option<usize>)>,
        output_buffer: &mut String,
        config: &config::Config,
        prefix: &str,
        style: Style,          // style for right fill if line contains no emph sections
        non_emph_style: Style, // style for right fill if line contains emph sections
        background_color_extends_to_terminal_width: Option<bool>,
    ) {
        // There's some unfortunate hackery going on here for two reasons:
        //
        // 1. The prefix needs to be injected into the output stream. We paint
        //    this with whatever style the line starts with.
        //
        // 2. We must ensure that we fill rightwards with the appropriate
        //    non-emph background color. In that case we don't use the last
        //    style of the line, because this might be emph.

        for ((syntax_sections, diff_sections), line_numbers) in syntax_style_sections
            .iter()
            .zip(diff_style_sections.iter())
            .zip(line_number_sections.iter())
        {
            let non_emph_style = if style_sections_contain_more_than_one_style(diff_sections) {
                non_emph_style // line contains an emph section
            } else {
                style
            };
            let mut ansi_strings = Vec::new();
            let mut handled_prefix = false;
            if config.show_line_numbers && has_line_numbers(line_numbers) {
                let (minus, plus) = line_numbers;
                let (minus_before, minus_number, minus_after) =
                    get_line_number_components(*minus, &config.number_minus_format);
                let (plus_before, plus_number, plus_after) =
                    get_line_number_components(*plus, &config.number_plus_format);

                ansi_strings.push(
                    config
                        .number_minus_format_style
                        .ansi_term_style
                        .paint(minus_before),
                );
                ansi_strings.push(
                    config
                        .number_minus_style
                        .ansi_term_style
                        .paint(minus_number),
                );
                ansi_strings.push(
                    config
                        .number_minus_format_style
                        .ansi_term_style
                        .paint(minus_after),
                );
                ansi_strings.push(
                    config
                        .number_plus_format_style
                        .ansi_term_style
                        .paint(plus_before),
                );
                ansi_strings.push(config.number_plus_style.ansi_term_style.paint(plus_number));
                ansi_strings.push(
                    config
                        .number_plus_format_style
                        .ansi_term_style
                        .paint(plus_after),
                );
            }
            for (section_style, mut text) in superimpose_style_sections(
                syntax_sections,
                diff_sections,
                config.true_color,
                config.null_syntect_style,
            ) {
                if !handled_prefix {
                    if prefix != "" {
                        ansi_strings.push(section_style.ansi_term_style.paint(prefix));
                        if text.len() > 0 {
                            text.remove(0);
                        }
                    }
                    handled_prefix = true;
                }
                ansi_strings.push(section_style.ansi_term_style.paint(text));
            }
            // Set style for the right-fill.
            let mut have_background_for_right_fill = false;
            if non_emph_style.ansi_term_style.background.is_some() {
                ansi_strings.push(non_emph_style.ansi_term_style.paint(""));
                have_background_for_right_fill = true;
            }
            let line = &mut ansi_term::ANSIStrings(&ansi_strings).to_string();
            let background_color_extends_to_terminal_width =
                match background_color_extends_to_terminal_width {
                    Some(boolean) => boolean,
                    None => config.background_color_extends_to_terminal_width,
                };
            if background_color_extends_to_terminal_width && have_background_for_right_fill {
                // HACK: How to properly incorporate the ANSI_CSI_ERASE_IN_LINE into ansi_strings?
                if line
                    .to_lowercase()
                    .ends_with(&ANSI_SGR_RESET.to_lowercase())
                {
                    line.truncate(line.len() - ANSI_SGR_RESET.len());
                }
                output_buffer.push_str(&line);
                output_buffer.push_str(ANSI_CSI_ERASE_IN_LINE);
                output_buffer.push_str(ANSI_SGR_RESET);
            } else {
                output_buffer.push_str(&line);
            }
            output_buffer.push_str("\n");
        }
    }

    /// Write output buffer to output stream, and clear the buffer.
    pub fn emit(&mut self) -> std::io::Result<()> {
        write!(self.writer, "{}", self.output_buffer)?;
        self.output_buffer.clear();
        Ok(())
    }

    pub fn should_compute_syntax_highlighting(state: &State, config: &config::Config) -> bool {
        if config.theme.is_none() {
            return false;
        }
        match state {
            State::HunkMinus => {
                config.minus_style.is_syntax_highlighted
                    || config.minus_emph_style.is_syntax_highlighted
            }
            State::HunkZero => config.zero_style.is_syntax_highlighted,
            State::HunkPlus => {
                config.plus_style.is_syntax_highlighted
                    || config.plus_emph_style.is_syntax_highlighted
            }
            State::HunkHeader => true,
            _ => panic!(
                "should_compute_syntax_highlighting is undefined for state {:?}",
                state
            ),
        }
    }

    pub fn get_syntax_style_sections_for_lines<'s>(
        lines: &'s Vec<String>,
        state: &State,
        highlighter: &mut HighlightLines,
        config: &config::Config,
    ) -> Vec<Vec<(SyntectStyle, &'s str)>> {
        let fake = !Painter::should_compute_syntax_highlighting(state, config);
        let mut line_sections = Vec::new();
        for line in lines.iter() {
            if fake {
                line_sections.push(vec![(config.null_syntect_style, line.as_str())])
            } else {
                line_sections.push(highlighter.highlight(line, &config.syntax_set))
            }
        }
        line_sections
    }

    /// Set background styles to represent diff for minus and plus lines in buffer.
    fn get_diff_style_sections<'b>(
        minus_lines: &'b Vec<String>,
        plus_lines: &'b Vec<String>,
        config: &config::Config,
    ) -> (Vec<Vec<(Style, &'b str)>>, Vec<Vec<(Style, &'b str)>>) {
        let mut diff_sections = edits::infer_edits(
            minus_lines,
            plus_lines,
            config.minus_style,
            config.minus_emph_style,
            config.plus_style,
            config.plus_emph_style,
            config.max_line_distance,
            config.max_line_distance_for_naively_paired_lines,
        );
        if config.minus_non_emph_style != config.minus_emph_style {
            Self::set_non_emph_styles(&mut diff_sections.0, config.minus_non_emph_style);
        }
        if config.plus_non_emph_style != config.plus_emph_style {
            Self::set_non_emph_styles(&mut diff_sections.1, config.plus_non_emph_style);
        }
        diff_sections
    }

    fn set_non_emph_styles(style_sections: &mut Vec<Vec<(Style, &str)>>, non_emph_style: Style) {
        for line_sections in style_sections {
            // If there multiple diff styles in the line, then the line must have some inferred
            // edit operations and so the non-emph color style should be used for the non-emph
            // style sections.
            if style_sections_contain_more_than_one_style(line_sections) {
                for section in line_sections.iter_mut() {
                    if !section.0.is_emph {
                        *section = (non_emph_style, section.1);
                    }
                }
            }
        }
    }
}

// edits::annotate doesn't return "coalesced" annotations (see comment there), so we can't assume
// that `sections.len() > 1 <=> (multiple styles)`.
fn style_sections_contain_more_than_one_style(sections: &Vec<(Style, &str)>) -> bool {
    if sections.len() > 1 {
        let (first_style, _) = sections[0];
        sections
            .iter()
            .filter(|(style, _)| *style != first_style)
            .next()
            .is_some()
    } else {
        false
    }
}

mod superimpose_style_sections {
    use syntect::highlighting::Style as SyntectStyle;

    use crate::bat::terminal::to_ansi_color;
    use crate::style::Style;

    pub fn superimpose_style_sections(
        sections_1: &[(SyntectStyle, &str)],
        sections_2: &[(Style, &str)],
        true_color: bool,
        null_syntect_style: SyntectStyle,
    ) -> Vec<(Style, String)> {
        coalesce(
            superimpose(
                explode(sections_1)
                    .iter()
                    .zip(explode(sections_2))
                    .collect::<Vec<(&(SyntectStyle, char), (Style, char))>>(),
            ),
            true_color,
            null_syntect_style,
        )
    }

    fn explode<T>(style_sections: &[(T, &str)]) -> Vec<(T, char)>
    where
        T: Copy,
    {
        let mut exploded: Vec<(T, char)> = Vec::new();
        for (style, s) in style_sections {
            for c in s.chars() {
                exploded.push((*style, c));
            }
        }
        exploded
    }

    fn superimpose(
        style_section_pairs: Vec<(&(SyntectStyle, char), (Style, char))>,
    ) -> Vec<((SyntectStyle, Style), char)> {
        let mut superimposed: Vec<((SyntectStyle, Style), char)> = Vec::new();
        for ((syntax_style, char_1), (style, char_2)) in style_section_pairs {
            if *char_1 != char_2 {
                panic!(
                    "String mismatch encountered while superimposing style sections: '{}' vs '{}'",
                    *char_1, char_2
                )
            }
            superimposed.push(((*syntax_style, style), *char_1));
        }
        superimposed
    }

    fn coalesce(
        style_sections: Vec<((SyntectStyle, Style), char)>,
        true_color: bool,
        null_syntect_style: SyntectStyle,
    ) -> Vec<(Style, String)> {
        let make_superimposed_style = |(syntect_style, style): (SyntectStyle, Style)| {
            if style.is_syntax_highlighted && syntect_style != null_syntect_style {
                Style {
                    ansi_term_style: ansi_term::Style {
                        foreground: Some(to_ansi_color(syntect_style.foreground, true_color)),
                        ..style.ansi_term_style
                    },
                    ..style
                }
            } else {
                style
            }
        };
        let mut coalesced: Vec<(Style, String)> = Vec::new();
        let mut style_sections = style_sections.iter();
        if let Some((style_pair, c)) = style_sections.next() {
            let mut current_string = c.to_string();
            let mut current_style_pair = style_pair;
            for (style_pair, c) in style_sections {
                if style_pair != current_style_pair {
                    let style = make_superimposed_style(*current_style_pair);
                    coalesced.push((style, current_string));
                    current_string = String::new();
                    current_style_pair = style_pair;
                }
                current_string.push(*c);
            }

            // TODO: This is not the ideal location for the following code.
            if current_string.ends_with("\n") {
                // Remove the terminating newline whose presence was necessary for the syntax
                // highlighter to work correctly.
                current_string.truncate(current_string.len() - 1);
            }
            let style = make_superimposed_style(*current_style_pair);
            coalesced.push((style, current_string));
        }
        coalesced
    }

    #[cfg(test)]
    mod tests {
        use lazy_static::lazy_static;

        use super::*;
        use ansi_term::{self, Color};
        use syntect::highlighting::Color as SyntectColor;
        use syntect::highlighting::FontStyle as SyntectFontStyle;
        use syntect::highlighting::Style as SyntectStyle;

        use crate::style::{DecorationStyle, Style};

        lazy_static! {
            static ref SYNTAX_STYLE: SyntectStyle = SyntectStyle {
                foreground: SyntectColor::BLACK,
                background: SyntectColor::BLACK,
                font_style: SyntectFontStyle::BOLD,
            };
        }
        lazy_static! {
            static ref SYNTAX_HIGHLIGHTED_STYLE: Style = Style {
                ansi_term_style: ansi_term::Style {
                    foreground: Some(Color::White),
                    background: Some(Color::White),
                    is_underline: true,
                    ..ansi_term::Style::new()
                },
                is_emph: false,
                is_omitted: false,
                is_raw: false,
                is_syntax_highlighted: true,
                decoration_style: DecorationStyle::NoDecoration,
            };
        }
        lazy_static! {
            static ref NON_SYNTAX_HIGHLIGHTED_STYLE: Style = Style {
                ansi_term_style: ansi_term::Style {
                    foreground: Some(Color::White),
                    background: Some(Color::White),
                    is_underline: true,
                    ..ansi_term::Style::new()
                },
                is_emph: false,
                is_omitted: false,
                is_raw: false,
                is_syntax_highlighted: false,
                decoration_style: DecorationStyle::NoDecoration,
            };
        }
        lazy_static! {
            static ref SUPERIMPOSED_STYLE: Style = Style {
                ansi_term_style: ansi_term::Style {
                    foreground: Some(to_ansi_color(SyntectColor::BLACK, true)),
                    background: Some(Color::White),
                    is_underline: true,
                    ..ansi_term::Style::new()
                },
                is_emph: false,
                is_omitted: false,
                is_raw: false,
                is_syntax_highlighted: true,
                decoration_style: DecorationStyle::NoDecoration,
            };
        }

        #[test]
        fn test_superimpose_style_sections_1() {
            let sections_1 = vec![(*SYNTAX_STYLE, "ab")];
            let sections_2 = vec![(*SYNTAX_HIGHLIGHTED_STYLE, "ab")];
            let superimposed = vec![(*SUPERIMPOSED_STYLE, "ab".to_string())];
            assert_eq!(
                superimpose_style_sections(&sections_1, &sections_2, true, SyntectStyle::default()),
                superimposed
            );
        }

        #[test]
        fn test_superimpose_style_sections_2() {
            let sections_1 = vec![(*SYNTAX_STYLE, "ab")];
            let sections_2 = vec![
                (*SYNTAX_HIGHLIGHTED_STYLE, "a"),
                (*SYNTAX_HIGHLIGHTED_STYLE, "b"),
            ];
            let superimposed = vec![(*SUPERIMPOSED_STYLE, String::from("ab"))];
            assert_eq!(
                superimpose_style_sections(&sections_1, &sections_2, true, SyntectStyle::default()),
                superimposed
            );
        }

        #[test]
        fn test_superimpose_style_sections_3() {
            let sections_1 = vec![(*SYNTAX_STYLE, "ab")];
            let sections_2 = vec![(*NON_SYNTAX_HIGHLIGHTED_STYLE, "ab")];
            let superimposed = vec![(*NON_SYNTAX_HIGHLIGHTED_STYLE, "ab".to_string())];
            assert_eq!(
                superimpose_style_sections(&sections_1, &sections_2, true, SyntectStyle::default()),
                superimposed
            );
        }

        #[test]
        fn test_explode() {
            let arbitrary = 0;
            assert_eq!(
                explode(&vec![(arbitrary, "ab")]),
                vec![(arbitrary, 'a'), (arbitrary, 'b')]
            )
        }

        #[test]
        fn test_superimpose() {
            let x = (*SYNTAX_STYLE, 'a');
            let pairs = vec![(&x, (*SYNTAX_HIGHLIGHTED_STYLE, 'a'))];
            assert_eq!(
                superimpose(pairs),
                vec![((*SYNTAX_STYLE, *SYNTAX_HIGHLIGHTED_STYLE), 'a')]
            );
        }
    }
}

lazy_static! {
    static ref LINE_NUMBER_REGEXP: Regex =
        Regex::new(r"(?P<before>.*)(?P<ln>%ln)(?P<after>.*)").unwrap();
}

fn format_line_number(line_number: Option<usize>) -> String {
    match line_number {
        Some(x) => format!("{:^4}", x),
        None => format!("    "),
    }
}

fn get_line_number_components(
    number: Option<usize>,
    number_format: &str,
) -> (String, String, String) {
    let caps = LINE_NUMBER_REGEXP.captures(number_format).unwrap();
    let before = caps.name("before").unwrap().as_str();
    let _ = caps.name("ln").unwrap();
    let after = caps.name("after").unwrap().as_str();
    (
        before.to_string(),
        format_line_number(number),
        after.to_string(),
    )
}

fn has_line_numbers(line_numbers: &(Option<usize>, Option<usize>)) -> bool {
    let (minus, plus) = line_numbers;
    match minus {
        Some(_) => true,
        None => match plus {
            Some(_) => true,
            None => false,
        },
    }
}

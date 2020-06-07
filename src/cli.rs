use std::path::PathBuf;
use std::process;

use git2;
use structopt::clap::AppSettings::{ColorAlways, ColoredHelp, DeriveDisplayOrder};
use structopt::{clap, StructOpt};

use crate::bat::assets::HighlightingAssets;
use crate::bat::output::PagingMode;
use crate::config;
use crate::env;
use crate::rewrite;
use crate::theme;

#[derive(StructOpt, Clone, Debug, PartialEq)]
#[structopt(
    name = "delta",
    about = "A syntax-highlighter for git and diff output",
    setting(ColorAlways),
    setting(ColoredHelp),
    setting(DeriveDisplayOrder),
    after_help = "\
STYLES
------

All options that have a name like --*-style work the same way. It is very similar to how
colors/styles are specified in a gitconfig file:
https://git-scm.com/docs/git-config#Documentation/git-config.txt-color

Here is an example:

--minus-style 'red bold ul #ffeeee'

That means: For removed lines, set the foreground (text) color to 'red', make it bold and
            underlined, and set the background color to '#ffeeee'.

See the COLORS section below for how to specify a color. In addition to real colors, there are 4
special color names: 'auto', 'normal', 'raw', and 'syntax'.

Here is an example of using special color names together with a single attribute:

--minus-style 'syntax bold auto'

That means: For removed lines, syntax-highlight the text, and make it bold, and do whatever delta
            normally does for the background.

The available attributes are: 'blink', 'bold', 'dim', 'hidden', 'italic', 'reverse', 'strike',
and 'ul' (or 'underline').

A complete description of the style string syntax follows:

- If the input that delta is receiving already has colors, and you want delta to output those
  colors unchanged, then use the special style string 'raw'. Otherwise, delta will strip any colors
  from its input.

- A style string consists of 0, 1, or 2 colors, together with an arbitrary number of style
  attributes, all separated by spaces.

- The first color is the foreground (text) color. The second color is the background color.
  Attributes can go in any position.

- This means that in order to specify a background color you must also specify a foreground (text)
  color.

- If you want delta to choose one of the colors automatically, then use the special color 'auto'.
  This can be used for both foreground and background.

- If you want the foreground/background color to be your terminal's foreground/background color,
  then use the special color 'normal'.

- If you want the foreground text to be syntax-highlighted according to its language, then use the
  special foreground color 'syntax'. This can only be used for the foreground (text).

- The minimal style specification is the empty string ''. This means: do not apply any colors or
  styling to the element in question.

COLORS
------

There are three ways to specify a color (this section applies to foreground and background colors
within a style string):

1. RGB hex code

   An example of using an RGB hex code is:
   --file-style=\"#0e7c0e\"

2. ANSI color name

   There are 8 ANSI color names:
   black, red, green, yellow, blue, magenta, cyan, white.

   In addition, all of them have a bright form:
   brightblack, brightred, brightgreen, brightyellow, brightblue, brightmagenta, brightcyan, brightwhite.

   An example of using an ANSI color name is:
   --file-style=\"green\"

   Unlike RGB hex codes, ANSI color names are just names: you can choose the exact color that each
   name corresponds to in the settings of your terminal application (the application you use to
   enter commands at a shell prompt). This means that if you use ANSI color names, and you change
   the color theme used by your terminal, then delta's colors will respond automatically, without
   needing to change the delta command line.

   \"purple\" is accepted as a synonym for \"magenta\". Color names and codes are case-insensitive.

3. ANSI color number

   An example of using an ANSI color number is:
   --file-style=28

   There are 256 ANSI color numbers: 0-255. The first 16 are the same as the colors described in
   the \"ANSI color name\" section above. See https://en.wikipedia.org/wiki/ANSI_escape_code#8-bit.
   Specifying colors like this is useful if your terminal only supports 256 colors (i.e. doesn\'t
   support 24-bit color).

LINE NUMBERS
------------

Options that have a name like --*-format allow you to specify a string to display for the line
number columns. The string should specify the location of the line number using the placeholder
%ln, and may optionally include supported formatting elements.

For example, to display the line numbers center-aligned, padded 4 places, and using special dividing
characters:

    8 ⋮   9 │ Here is an output line
    9 ⋮  10 │ Here is another output line
   10 ⋮  11 │ Here is the line number

you would use the following input:

--number-minus-format '%^4%ln ⋮'
--number-plus-format '%^4%ln │'

String formatting elements such as column width, alignment, zero-padding, etc. that are supported
by Rust should be rendered as specified here https://doc.rust-lang.org/std/fmt/.

If something isn't working correctly, or you have a feature request, please open an issue at
https://github.com/dandavison/delta/issues.
"
)]
pub struct Opt {
    #[structopt(long = "theme", env = "BAT_THEME")]
    /// The code syntax highlighting theme to use. Use --list-themes to demo available themes. If
    /// the theme is not set using this option, it will be taken from the BAT_THEME environment
    /// variable, if that contains a valid theme name. --theme=none disables all syntax
    /// highlighting.
    pub theme: Option<String>,

    /// Use default colors appropriate for a light terminal background. For more control, see the
    /// style options.
    #[structopt(long = "light")]
    pub light: bool,

    /// Use default colors appropriate for a dark terminal background. For more control, see the
    /// style options.
    #[structopt(long = "dark")]
    pub dark: bool,

    #[structopt(long = "minus-style", default_value = "normal auto")]
    /// Style (foreground, background, attributes) for removed lines. See STYLES section.
    pub minus_style: String,

    #[structopt(long = "zero-style", default_value = "syntax normal")]
    /// Style (foreground, background, attributes) for unchanged lines. See STYLES section.
    pub zero_style: String,

    #[structopt(long = "plus-style", default_value = "syntax auto")]
    /// Style (foreground, background, attributes) for added lines. See STYLES section.
    pub plus_style: String,

    #[structopt(long = "minus-emph-style", default_value = "normal auto")]
    /// Style (foreground, background, attributes) for emphasized sections of removed lines. See
    /// STYLES section.
    pub minus_emph_style: String,

    #[structopt(long = "minus-non-emph-style", default_value = "auto auto")]
    /// Style (foreground, background, attributes) for non-emphasized sections of removed lines
    /// that have an emphasized section. Defaults to --minus-style. See STYLES section.
    pub minus_non_emph_style: String,

    #[structopt(long = "plus-emph-style", default_value = "syntax auto")]
    /// Style (foreground, background, attributes) for emphasized sections of added lines. See
    /// STYLES section.
    pub plus_emph_style: String,

    #[structopt(long = "plus-non-emph-style", default_value = "auto auto")]
    /// Style (foreground, background, attributes) for non-emphasized sections of added lines that
    /// have an emphasized section. Defaults to --plus-style. See STYLES section.
    pub plus_non_emph_style: String,

    #[structopt(long = "commit-style", default_value = "raw")]
    /// Style (foreground, background, attributes) for the commit hash line. See STYLES section.
    pub commit_style: String,

    #[structopt(long = "commit-decoration-style", default_value = "")]
    /// Style (foreground, background, attributes) for the commit hash decoration. See STYLES
    /// section. One of the special attributes 'box', 'ul', 'overline', or 'underoverline' must be
    /// given.
    pub commit_decoration_style: String,

    #[structopt(long = "file-style", default_value = "blue")]
    /// Style (foreground, background, attributes) for the file section. See STYLES section.
    pub file_style: String,

    #[structopt(long = "file-decoration-style", default_value = "blue ul")]
    /// Style (foreground, background, attributes) for the file decoration. See STYLES section. One
    /// of the special attributes 'box', 'ul', 'overline', or 'underoverline' must be given.
    pub file_decoration_style: String,

    #[structopt(long = "emulate-diff-highlight")]
    /// Emulate diff-highlight (https://github.com/git/git/tree/master/contrib/diff-highlight)
    pub emulate_diff_highlight: bool,

    #[structopt(long = "emulate-diff-so-fancy")]
    /// Emulate diff-so-fancy (https://github.com/so-fancy/diff-so-fancy)
    pub emulate_diff_so_fancy: bool,

    #[structopt(long = "navigate")]
    /// Activate diff navigation: use n to jump forwards and N to jump backwards. To change the
    /// file labels used see --file-modified-label, --file-removed-label, --file-added-label,
    /// --file-renamed-label.
    pub navigate: bool,

    #[structopt(long = "file-modified-label", default_value = "")]
    /// Text to display in front of a modified file path.
    pub file_modified_label: String,

    #[structopt(long = "file-removed-label", default_value = "removed:")]
    /// Text to display in front of a removed file path.
    pub file_removed_label: String,

    #[structopt(long = "file-added-label", default_value = "added:")]
    /// Text to display in front of a added file path.
    pub file_added_label: String,

    #[structopt(long = "file-renamed-label", default_value = "renamed:")]
    /// Text to display in front of a renamed file path.
    pub file_renamed_label: String,

    #[structopt(long = "hunk-header-style", default_value = "syntax")]
    /// Style (foreground, background, attributes) for the hunk-header. See STYLES section.
    pub hunk_header_style: String,

    #[structopt(long = "hunk-header-decoration-style", default_value = "blue box")]
    /// Style (foreground, background, attributes) for the hunk-header decoration. See STYLES
    /// section. One of the special attributes 'box', 'ul', 'overline', or 'underoverline' must be
    /// given.
    pub hunk_header_decoration_style: String,

    /// Display line numbers next to the diff. The first column contains line
    /// numbers in the previous version of the file, and the second column contains
    /// line number in the new version of the file. A blank cell in the first or
    /// second column indicates that the line does not exist in that file (it was
    /// added or removed, respectively).
    #[structopt(short = "n", long = "number")]
    pub show_line_numbers: bool,

    /// Style (foreground, background, attributes) for the left (minus) column of line numbers
    /// (--number), if --number is set. See STYLES section.
    /// Defaults to --hunk-style.
    #[structopt(long = "number-minus-style", default_value = "")]
    pub number_minus_style: String,

    /// Style (foreground, background, attributes) for the right (plus) column of line numbers
    /// (--number), if --number is set. See STYLES section.
    /// Defaults to --hunk-style.
    #[structopt(long = "number-plus-style", default_value = "")]
    pub number_plus_style: String,

    /// Format string for the left (minus) column of line numbers (--number), if --number is set.
    /// Should include the placeholder %ln to indicate the position of the line number.
    /// See the LINE NUMBERS section.
    /// Defaults to '%ln⋮'
    #[structopt(long = "number-minus-format", default_value = "%ln⋮")]
    pub number_minus_format: String,

    /// Format string for the right (plus) column of line numbers (--number), if --number is set.
    /// Should include the placeholder %ln to indicate the position of the line number.
    /// See the LINE NUMBERS section.
    /// Defaults to '%ln│ '
    #[structopt(long = "number-plus-format", default_value = "%ln│ ")]
    pub number_plus_format: String,

    /// Style (foreground, background, attributes) for the left (minus) line number format string
    /// (--number), if --number is set. See STYLES section.
    /// Defaults to --hunk-style.
    #[structopt(long = "number-minus-format-style", default_value = "")]
    pub number_minus_format_style: String,

    /// Style (foreground, background, attributes) for the right (plus) line number format string
    /// (--number), if --number is set. See STYLES section.
    /// Defaults to --hunk-style.
    #[structopt(long = "number-plus-format-style", default_value = "")]
    pub number_plus_format_style: String,

    #[structopt(long = "color-only")]
    /// Do not alter the input in any way other than applying colors. Equivalent to
    /// `--keep-plus-minus-markers --width variable --tabs 0 --commit-decoration ''
    /// --file-decoration '' --hunk-decoration ''`.
    pub color_only: bool,

    #[structopt(long = "keep-plus-minus-markers")]
    /// Prefix added/removed lines with a +/- character, respectively, exactly as git does. The
    /// default behavior is to output a space character in place of these markers.
    pub keep_plus_minus_markers: bool,

    /// The width of underline/overline decorations. Use --width=variable to extend decorations and
    /// background colors to the end of the text only. Otherwise background colors extend to the
    /// full terminal width.
    #[structopt(short = "w", long = "width")]
    pub width: Option<String>,

    /// The number of spaces to replace tab characters with. Use --tabs=0 to pass tab characters
    /// through directly, but note that in that case delta will calculate line widths assuming tabs
    /// occupy one character's width on the screen: if your terminal renders tabs as more than than
    /// one character wide then delta's output will look incorrect.
    #[structopt(long = "tabs", default_value = "4")]
    pub tab_width: usize,

    /// Show the command-line arguments (RGB hex codes) for the background colors that are in
    /// effect. The hex codes are displayed with their associated background color. This option can
    /// be combined with --light and --dark to view the background colors for those modes. It can
    /// also be used to experiment with different RGB hex codes by combining this option with style
    /// options such as --minus-style, --zero-style, --plus-style, etc.
    #[structopt(long = "show-background-colors")]
    pub show_background_colors: bool,

    /// List supported languages and associated file extensions.
    #[structopt(long = "list-languages")]
    pub list_languages: bool,

    /// List available syntax-highlighting color themes.
    #[structopt(long = "list-theme-names")]
    pub list_theme_names: bool,

    /// List available syntax highlighting themes, each with an example of highlighted diff output.
    /// If diff output is supplied on standard input then this will be used for the demo. For
    /// example: `git show --color=always | delta --list-themes`.
    #[structopt(long = "list-themes")]
    pub list_themes: bool,

    /// The maximum distance between two lines for them to be inferred to be homologous. Homologous
    /// line pairs are highlighted according to the deletion and insertion operations transforming
    /// one into the other.
    #[structopt(long = "max-line-distance", default_value = "0.6")]
    pub max_line_distance: f64,

    /// Whether to emit 24-bit ("true color") RGB color codes. Options are auto, always, and never.
    /// "auto" means that delta will emit 24-bit color codes iff the environment variable COLORTERM
    /// has the value "truecolor" or "24bit". If your terminal application (the application you use
    /// to enter commands at a shell prompt) supports 24 bit colors, then it probably already sets
    /// this environment variable, in which case you don't need to do anything.
    #[structopt(long = "24-bit-color", default_value = "auto")]
    pub true_color: String,

    /// Whether to use a pager when displaying output. Options are: auto, always, and never. The
    /// default pager is `less`: this can be altered by setting the environment variables BAT_PAGER
    /// or PAGER (BAT_PAGER has priority).
    #[structopt(long = "paging", default_value = "auto")]
    pub paging_mode: String,

    /// First file to be compared when delta is being used in diff mode.
    #[structopt(parse(from_os_str))]
    pub minus_file: Option<PathBuf>,

    /// Second file to be compared when delta is being used in diff mode.
    #[structopt(parse(from_os_str))]
    pub plus_file: Option<PathBuf>,

    #[structopt(long = "minus-color")]
    /// Deprecated: use --minus-style='normal my_background_color'.
    pub deprecated_minus_background_color: Option<String>,

    #[structopt(long = "minus-emph-color")]
    /// Deprecated: use --minus-emph-style='normal my_background_color'.
    pub deprecated_minus_emph_background_color: Option<String>,

    #[structopt(long = "plus-color")]
    /// Deprecated: Use --plus-style='syntax my_background_color' to change the background color
    /// while retaining syntax-highlighting.
    pub deprecated_plus_background_color: Option<String>,

    #[structopt(long = "plus-emph-color")]
    /// Deprecated: Use --plus-emph-style='syntax my_background_color' to change the background
    /// color while retaining syntax-highlighting.
    pub deprecated_plus_emph_background_color: Option<String>,

    #[structopt(long = "highlight-removed")]
    /// Deprecated: use --minus-style='syntax'.
    pub deprecated_highlight_minus_lines: bool,

    #[structopt(long = "commit-color")]
    /// Deprecated: use --commit-style='my_foreground_color'
    /// --commit-decoration-style='my_foreground_color'.
    pub deprecated_commit_color: Option<String>,

    #[structopt(long = "file-color")]
    /// Deprecated: use --file-style='my_foreground_color'
    /// --file-decoration-style='my_foreground_color'.
    pub deprecated_file_color: Option<String>,

    #[structopt(long = "hunk-style")]
    /// Deprecated: synonym of --hunk-header-decoration-style.
    pub deprecated_hunk_style: Option<String>,

    #[structopt(long = "hunk-color")]
    /// Deprecated: use --hunk-header-style='my_foreground_color'
    /// --hunk-header-decoration-style='my_foreground_color'.
    pub deprecated_hunk_color: Option<String>,
}

pub fn process_command_line_arguments<'a>(
    mut opt: Opt,
    arg_matches: Option<clap::ArgMatches>,
) -> config::Config<'a> {
    let assets = HighlightingAssets::new();

    _check_validity(&opt, &assets);

    let mut git_config = match std::env::current_dir() {
        Ok(dir) => match git2::Repository::discover(dir) {
            Ok(repo) => match repo.config() {
                Ok(config) => Some(config),
                Err(_) => None,
            },
            Err(_) => None,
        },
        Err(_) => None,
    };

    rewrite::apply_rewrite_rules(&mut opt, &mut git_config, arg_matches);

    let paging_mode = match opt.paging_mode.as_ref() {
        "always" => PagingMode::Always,
        "never" => PagingMode::Never,
        "auto" => PagingMode::QuitIfOneScreen,
        _ => {
            eprintln!(
                "Invalid value for --paging option: {} (valid values are \"always\", \"never\", and \"auto\")",
                opt.paging_mode
            );
            process::exit(1);
        }
    };

    let true_color = match opt.true_color.as_ref() {
        "always" => true,
        "never" => false,
        "auto" => is_truecolor_terminal(),
        _ => {
            eprintln!(
                "Invalid value for --24-bit-color option: {} (valid values are \"always\", \"never\", and \"auto\")",
                opt.true_color
            );
            process::exit(1);
        }
    };

    config::get_config(
        opt,
        assets.syntax_set,
        assets.theme_set,
        true_color,
        paging_mode,
    )
}

/// If `opt_name` was not supplied on the command line, then change its value to one of the
/// following in order of precedence:
/// 1. The entry for it in the section of gitconfig correspnding to the selected theme, if there is
///    one.
/// 2. The entry for it in the main delta section of gitconfig, if there is one.
/// 3. The default value passed to this macro (which may be the current value).
macro_rules! set_options {
    ([$( ($opt_name:expr, $field_ident:ident, $keys:expr, $default:expr) ),* ],
     $opt:expr, $git_config:expr, $arg_matches:expr) => {
        $(
            if $arg_matches.is_none() || !user_supplied_option($opt_name, $arg_matches.unwrap()) {
                $opt.$field_ident =
                    value_from_git_config($keys, $git_config)
                    .unwrap_or_else(|| $default.to_string());
            };
        )*
    };
    // This second rule handles a trailing comma.
    ([$(($a:expr, $b:ident, $c:expr, $d:expr)),* ,], $e:expr, $f:expr, $g:expr) => {
        set_options!([$( ($a, $b, $c, $d) ),*], $e, $f, $g);
    };
}

macro_rules! set_delta_options {
	([$( ($option_name:expr, $field_ident:ident) ),* ],
     $opt:expr, $git_config:expr, $arg_matches:expr) => {
		set_options!([
            $(
                ($option_name,
                 $field_ident,
                 cli::make_git_config_keys_for_delta($option_name, $opt.theme.as_deref()),
                 &$opt.$field_ident)
            ),*
        ],
        $opt,
        $git_config,
        $arg_matches);
	};
}

/// Did the user supply `option` on the command line?
pub fn user_supplied_option(option: &str, arg_matches: &clap::ArgMatches) -> bool {
    arg_matches.occurrences_of(option) > 0
}

// TODO: user_supplied_option should be used to give precedence to command line options over
// gitconfig.
pub fn value_from_git_config(
    keys: Vec<String>,
    git_config: &mut Option<git2::Config>,
) -> Option<String> {
    match git_config {
        Some(git_config) => {
            let git_config = git_config.snapshot().unwrap();
            for key in keys {
                let entry = git_config.get_str(&key);
                if let Ok(entry) = entry {
                    return Some(entry.to_string());
                }
            }
            return None;
        }
        None => None,
    }
}

pub fn make_git_config_keys_for_delta(key: &str, theme: Option<&str>) -> Vec<String> {
    match theme {
        Some(theme) => vec![format!("delta.{}.{}", theme, key), format!("delta.{}", key)],
        None => vec![format!("delta.{}", key)],
    }
}

fn _check_validity(opt: &Opt, assets: &HighlightingAssets) {
    if opt.light && opt.dark {
        eprintln!("--light and --dark cannot be used together.");
        process::exit(1);
    }
    if let Some(ref theme) = opt.theme {
        if !theme::is_no_syntax_highlighting_theme_name(&theme) {
            if !assets.theme_set.themes.contains_key(theme.as_str()) {
                return;
            }
            let is_light_theme = theme::is_light_theme(&theme);
            if is_light_theme && opt.dark {
                eprintln!(
                    "{} is a light theme, but you supplied --dark. \
                     If you use --theme, you do not need to supply --light or --dark.",
                    theme
                );
                process::exit(1);
            } else if !is_light_theme && opt.light {
                eprintln!(
                    "{} is a dark theme, but you supplied --light. \
                     If you use --theme, you do not need to supply --light or --dark.",
                    theme
                );
                process::exit(1);
            }
        }
    }
}

pub fn unreachable(message: &str) -> ! {
    eprintln!(
        "{} This should not be possible. \
         Please report the bug at https://github.com/dandavison/delta/issues.",
        message
    );
    process::exit(1);
}

fn is_truecolor_terminal() -> bool {
    env::get_env_var("COLORTERM")
        .map(|colorterm| colorterm == "truecolor" || colorterm == "24bit")
        .unwrap_or(false)
}

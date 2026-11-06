//! Capturing the screen a `hand` scenario names as a png in the task's notes
//! directory, whatever the screen was made of.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const MAX_TEXT_LINES: usize = 400;

/// Where the pixels of a screen come from.
pub enum Source {
    /// A visible X11 window, found by a substring of its title.
    Window(String),
    /// A file whose text is rendered as an image; `-` reads standard input.
    Text(PathBuf),
}

/// Write `<notes_dir>/screens/<scenario>.png` from `source`, creating `screens/` when it
/// is absent and replacing an earlier capture of the same scenario. On success prints
/// the path relative to `notes_dir` (`screens/S6.png`) and exits 0. On failure prints
/// one line to stderr, leaves no file behind and exits 1.
///
/// A window source needs `xdotool` and ImageMagick's `import`, an X server, and a
/// window that server has mapped: one on a tag that is not displayed does not capture.
/// A name that matches more than one visible window is refused rather than guessed at.
/// A text source needs ImageMagick's `convert`, and only the first 400 lines of the text
/// are drawn.
pub fn run(notes_dir: &Path, scenario: &str, source: Source) -> ExitCode {
    match capture(notes_dir, scenario, source) {
        Ok(relative) => {
            println!("{relative}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hcoord capture: {e}");
            ExitCode::from(1)
        }
    }
}

fn capture(notes_dir: &Path, scenario: &str, source: Source) -> Result<String, String> {
    let out = screen_path(notes_dir, scenario)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let result = match source {
        Source::Window(name) => capture_window(&name, &out),
        Source::Text(path) => read_text(&path).and_then(|text| render_text(&text, &out)),
    };
    if let Err(e) = result {
        let _ = std::fs::remove_file(&out);
        return Err(e);
    }
    Ok(format!("screens/{scenario}.png"))
}

fn screen_path(notes_dir: &Path, scenario: &str) -> Result<PathBuf, String> {
    let shaped = !scenario.is_empty()
        && !scenario.starts_with('.')
        && scenario
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !shaped {
        return Err(format!(
            "scenario id {scenario:?} is not a file name: use letters, digits, `.`, `_` and `-`"
        ));
    }
    Ok(notes_dir.join("screens").join(format!("{scenario}.png")))
}

fn clamp(text: &str, max_lines: usize) -> String {
    let total = text.lines().count();
    if total <= max_lines {
        return text.to_owned();
    }
    let mut kept: Vec<&str> = text.lines().take(max_lines).collect();
    let more = format!("… {} more lines", total - max_lines);
    kept.push(&more);
    kept.join("\n")
}

fn pick_window(stdout: &str) -> Result<String, String> {
    let ids: Vec<&str> = stdout.split_whitespace().collect();
    match ids.as_slice() {
        [] => Err("no visible window matches that name".to_owned()),
        [id] => Ok((*id).to_owned()),
        many => Err(format!(
            "{} visible windows match that name ({}); give a more specific one",
            many.len(),
            many.join(", ")
        )),
    }
}

fn imagemagick(tool: &str) -> (String, Vec<String>) {
    if on_path("magick") {
        ("magick".to_owned(), vec![tool.to_owned()])
    } else {
        (tool.to_owned(), Vec::new())
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn run_tool(program: &str, args: &[String]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr.lines().next().unwrap_or("").trim();
        return Err(format!("{program} failed ({}): {first}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn capture_window(name: &str, out: &Path) -> Result<(), String> {
    let found = run_tool(
        "xdotool",
        &[
            "search".to_owned(),
            "--onlyvisible".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
        ],
    )
    .map_err(|e| format!("looking for a window named {name:?}: {e}"))?;
    let id = pick_window(&found).map_err(|e| format!("window named {name:?}: {e}"))?;
    let (program, mut args) = imagemagick("import");
    args.extend(["-window".to_owned(), id, out.to_string_lossy().into_owned()]);
    run_tool(&program, &args).map(|_| ())
}

/// The text of the file at `path`, or of standard input, read to its end, when `path`
/// is `-`. `Err` is a message naming what could not be read.
pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    if path == Path::new("-") {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|e| format!("cannot read standard input: {e}"))?;
        return Ok(text);
    }
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn render_text(text: &str, out: &Path) -> Result<(), String> {
    let clamped = clamp(text, MAX_TEXT_LINES);
    let label = if clamped.trim().is_empty() {
        "(empty)".to_owned()
    } else {
        clamped.replace('%', "%%")
    };
    let label = if label.starts_with('@') {
        format!(" {label}")
    } else {
        label
    };
    let (program, base) = imagemagick("convert");
    let build = |with_font: bool| {
        let mut args = base.clone();
        args.extend(["-background".to_owned(), "white".to_owned()]);
        args.extend(["-fill".to_owned(), "black".to_owned()]);
        if with_font {
            args.extend(["-font".to_owned(), "DejaVu-Sans-Mono".to_owned()]);
        }
        args.extend(["-pointsize".to_owned(), "14".to_owned()]);
        args.push(format!("label:{label}"));
        args.push(out.to_string_lossy().into_owned());
        args
    };
    run_tool(&program, &build(true))
        .or_else(|_| run_tool(&program, &build(false)))
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The id becomes a path component, so anything that could leave `screens/` or hide as
    // a dotfile has to be refused before a file is written.
    #[test]
    fn a_scenario_id_that_is_not_a_file_name_is_refused() {
        let notes = Path::new("/n");
        assert_eq!(
            screen_path(notes, "S6").unwrap(),
            PathBuf::from("/n/screens/S6.png")
        );
        assert!(screen_path(notes, "../S6").is_err());
        assert!(screen_path(notes, "a/b").is_err());
        assert!(screen_path(notes, ".S6").is_err());
        assert!(screen_path(notes, "").is_err());
        assert!(screen_path(notes, "S 6").unwrap_err().contains("\"S 6\""));
    }

    // A whole log rendered at one pixel row per line is an image nobody can look at, so
    // the cut has to happen and has to say how much was left out.
    #[test]
    fn clamp_keeps_the_first_lines_and_says_how_many_it_cut() {
        assert_eq!(clamp("a\nb", 5), "a\nb");
        assert_eq!(clamp("a\nb\nc\nd", 2), "a\nb\n… 2 more lines");
    }

    // A capture of the wrong window is wrong silently, so an ambiguous name must be an
    // error that names every candidate, and no match must not be an empty success.
    #[test]
    fn pick_window_takes_one_id_and_refuses_none_or_several() {
        assert_eq!(pick_window("123\n").unwrap(), "123");
        assert!(pick_window("").is_err());
        let err = pick_window("1\n2\n").unwrap_err();
        assert!(err.contains("1, 2"), "{err}");
    }

    // ImageMagick 7 dropped the bare `import` and `convert` names in favour of `magick`;
    // which a machine has is discovered, so the result must be one of the two spellings.
    #[test]
    fn imagemagick_names_the_program_this_machine_has() {
        let (program, args) = imagemagick("import");
        if program == "magick" {
            assert_eq!(args, vec!["import".to_owned()]);
        } else {
            assert_eq!(program, "import");
            assert!(args.is_empty());
        }
    }

    // The whole text path against the real tool: what comes out must be a png, and text
    // that would be an ImageMagick escape (`%`, a leading `@`) must still render.
    #[test]
    fn rendered_text_is_a_png() {
        if !on_path("magick") && !on_path("convert") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("hcoord-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("screen.txt");
        std::fs::write(&source, "@ one 100%\ntwo\nthree\n").unwrap();
        let code = capture(&dir, "S6", Source::Text(source)).unwrap();
        assert_eq!(code, "screens/S6.png");
        let bytes = std::fs::read(dir.join("screens/S6.png")).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

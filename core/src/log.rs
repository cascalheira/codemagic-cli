//! Parsing Codemagic's build logs.
//!
//! The API serves logs as HTML, not plain text: echoed commands come wrapped
//! in `<span style="color:#268BD2">`, and the whole body is entity-escaped, so
//! a `>` prompt arrives as `&gt;`. Rendering the body verbatim shows the
//! markup, which is both ugly and unsearchable — a filter for "span" would
//! match every command line.
//!
//! So the markup is parsed rather than stripped: the colours are the log
//! telling us which lines are commands and which are output, and that's worth
//! keeping.

/// A run of log text sharing one colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    /// A validated CSS colour, or `None` for the default foreground.
    pub color: Option<String>,
}

/// One line of a log, split into coloured runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    pub segments: Vec<Segment>,
}

impl Line {
    /// The line as plain text, for searching, copying, and saving.
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Parses an HTML log body into lines of coloured segments.
///
/// Tool output inside the log (Gradle, Flutter, fastlane, …) often carries
/// raw ANSI escape sequences the API passes through verbatim; SGR colour
/// codes are honoured and every other escape sequence is stripped, so none
/// of it renders as `[32m` garbage. An ANSI colour, being the innermost
/// intent, wins over an enclosing HTML span's.
pub fn parse(html: &str) -> Vec<Line> {
    let mut lines = vec![Line::default()];
    // Nested spans are rare but legal; the innermost colour wins, and popping
    // restores the enclosing one.
    let mut colors: Vec<Option<String>> = Vec::new();
    // The current ANSI foreground, until a reset.
    let mut ansi: Option<String> = None;
    let mut buffer = String::new();
    let mut chars = html.chars().peekable();

    // Flushes `buffer` into the current line, splitting on newlines.
    macro_rules! flush {
        () => {
            if !buffer.is_empty() {
                let color = ansi.clone().or_else(|| colors.last().cloned().flatten());
                let mut parts = buffer.split('\n');
                if let Some(first) = parts.next()
                    && !first.is_empty()
                {
                    lines.last_mut().unwrap().segments.push(Segment {
                        text: first.to_string(),
                        color: color.clone(),
                    });
                }
                for part in parts {
                    lines.push(Line::default());
                    if !part.is_empty() {
                        lines.last_mut().unwrap().segments.push(Segment {
                            text: part.to_string(),
                            color: color.clone(),
                        });
                    }
                }
                buffer.clear();
            }
        };
    }

    while let Some(c) = chars.next() {
        match c {
            '<' => {
                flush!();
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                    tag.push(t);
                }
                let trimmed = tag.trim();
                if let Some(rest) = trimmed.strip_prefix('/') {
                    if rest.trim().eq_ignore_ascii_case("span") {
                        colors.pop();
                    }
                } else if trimmed.split([' ', '\t']).next().unwrap_or("") == "span" {
                    colors.push(color_of(trimmed));
                }
                // Any other tag (<br>, <b>, …) contributes no text.
            }
            '&' => {
                let mut entity = String::new();
                let mut terminated = false;
                // Entities are short; anything longer is a stray ampersand.
                while let Some(&t) = chars.peek() {
                    if t == ';' {
                        chars.next();
                        terminated = true;
                        break;
                    }
                    if entity.len() >= 8 || t == '<' || t == '&' || t.is_whitespace() {
                        break;
                    }
                    entity.push(t);
                    chars.next();
                }
                match decode(&entity) {
                    Some(decoded) => buffer.push_str(&decoded),
                    // Not an entity after all — put back exactly what was
                    // written, semicolon included, or `a&b;c` loses the `;`.
                    None => {
                        buffer.push('&');
                        buffer.push_str(&entity);
                        if terminated {
                            buffer.push(';');
                        }
                    }
                }
            }
            '\x1b' => {
                flush!();
                match chars.peek() {
                    // CSI: parameters up to a final byte in @–~; only SGR
                    // ("m", colours and styles) changes anything.
                    Some('[') => {
                        chars.next();
                        let mut seq = String::new();
                        while let Some(&t) = chars.peek() {
                            chars.next();
                            if ('\x40'..='\x7e').contains(&t) {
                                if t == 'm' {
                                    apply_sgr(&seq, &mut ansi);
                                }
                                break;
                            }
                            seq.push(t);
                        }
                    }
                    // OSC (window titles, hyperlinks): runs to BEL or ESC\.
                    Some(']') => {
                        chars.next();
                        while let Some(t) = chars.next() {
                            if t == '\x07' {
                                break;
                            }
                            if t == '\x1b' {
                                if chars.peek() == Some(&'\\') {
                                    chars.next();
                                }
                                break;
                            }
                        }
                    }
                    // Two-character escapes (charset selection etc.).
                    _ => {
                        chars.next();
                    }
                }
            }
            // Normalise CRLF so Windows-built logs don't render stray blanks.
            '\r' => {}
            _ => buffer.push(c),
        }
    }
    flush!();

    // A trailing newline shouldn't add a blank line to the display.
    if lines.last().is_some_and(|l| l.segments.is_empty()) {
        lines.pop();
    }
    lines
}

/// The whole log as plain text.
pub fn to_plain(html: &str) -> String {
    parse(html)
        .iter()
        .map(Line::text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The 8 basic ANSI foregrounds, as mid-tone hexes readable on both a white
/// and a near-black log background. Black maps to gray for the same reason.
const ANSI_BASIC: [&str; 8] = [
    "#8e8e93", "#e5484d", "#30a46c", "#b58900", "#3b82f6", "#bf5af2", "#0e9aa7", "#98989f",
];
const ANSI_BRIGHT: [&str; 8] = [
    "#a5a5aa", "#ff6b6b", "#3ecf7a", "#d0a215", "#5e9bff", "#d17bf5", "#25b8c8", "#c7c7cc",
];

/// Applies one SGR sequence's parameters to the current foreground colour.
///
/// Styles other than foreground colour (bold, underline, backgrounds) are
/// consumed and ignored — a log viewer needs the colour semantics, not a
/// full terminal emulation.
fn apply_sgr(seq: &str, color: &mut Option<String>) {
    let params: Vec<u16> = seq
        .split(';')
        .map(|p| p.parse::<u16>().unwrap_or(0))
        .collect();
    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 | 39 => *color = None,
            n @ 30..=37 => *color = Some(ANSI_BASIC[(n - 30) as usize].to_string()),
            n @ 90..=97 => *color = Some(ANSI_BRIGHT[(n - 90) as usize].to_string()),
            38 => match params.get(i + 1) {
                Some(5) => {
                    if let Some(&n) = params.get(i + 2) {
                        *color = Some(ansi_256(n.min(255) as u8));
                    }
                    i += 2;
                }
                Some(2) => {
                    if let (Some(&r), Some(&g), Some(&b)) =
                        (params.get(i + 2), params.get(i + 3), params.get(i + 4))
                    {
                        *color = Some(format!(
                            "#{:02x}{:02x}{:02x}",
                            r.min(255),
                            g.min(255),
                            b.min(255)
                        ));
                    }
                    i += 4;
                }
                _ => {}
            },
            // Background colours: consumed so their payload isn't misread as
            // more parameters, but never applied.
            48 => match params.get(i + 1) {
                Some(5) => i += 2,
                Some(2) => i += 4,
                _ => {}
            },
            _ => {}
        }
        i += 1;
    }
}

/// A hex colour for an xterm-256 palette index.
fn ansi_256(n: u8) -> String {
    match n {
        0..=7 => ANSI_BASIC[n as usize].to_string(),
        8..=15 => ANSI_BRIGHT[(n - 8) as usize].to_string(),
        // 6×6×6 colour cube.
        16..=231 => {
            let n = n - 16;
            let level = |c: u8| if c == 0 { 0 } else { 55 + c * 40 };
            format!(
                "#{:02x}{:02x}{:02x}",
                level(n / 36),
                level((n % 36) / 6),
                level(n % 6)
            )
        }
        // Grayscale ramp.
        _ => {
            let v = 8 + (n - 232) * 10;
            format!("#{v:02x}{v:02x}{v:02x}")
        }
    }
}

/// Extracts a `color:` declaration from a tag's attributes.
///
/// Only hex colours are accepted. The value ends up in a `style` attribute, so
/// anything else — `url(...)`, an unbalanced quote, a second declaration — is
/// dropped rather than passed through to the renderer.
fn color_of(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find("color:")? + "color:".len();
    let value: String = tag[at..]
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '#')
        .collect();

    let hex = value.strip_prefix('#')?;
    let valid = matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    valid.then(|| format!("#{hex}"))
}

/// Decodes one HTML entity's body (the part between `&` and `;`).
fn decode(entity: &str) -> Option<String> {
    let named = match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" => ' ',
        _ => {
            let digits = entity.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            return char::from_u32(code).map(String::from);
        }
    };
    Some(named.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Line, parse, to_plain};

    fn texts(html: &str) -> Vec<String> {
        parse(html).iter().map(Line::text).collect()
    }

    #[test]
    fn plain_text_survives_untouched() {
        assert_eq!(texts("one\ntwo"), ["one", "two"]);
    }

    /// The case from a real build log.
    #[test]
    fn a_coloured_command_line_keeps_its_colour_and_loses_its_markup() {
        let log = "== Use Xcode ==\n\
                   <span style=\"color:#268BD2\">&gt; xcodebuild -version</span>\n\
                   Xcode 26.4.1";
        assert_eq!(
            texts(log),
            ["== Use Xcode ==", "> xcodebuild -version", "Xcode 26.4.1"]
        );
        let lines = parse(log);
        assert_eq!(lines[1].segments[0].color.as_deref(), Some("#268BD2"));
        assert_eq!(lines[0].segments[0].color, None);
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(
            texts("&gt; gem list &#39;^cocoapods$&#39; &amp;&amp; echo &quot;ok&quot;"),
            ["> gem list '^cocoapods$' && echo \"ok\""]
        );
    }

    #[test]
    fn numeric_and_hex_entities_both_decode() {
        assert_eq!(texts("&#65;&#x42;"), ["AB"]);
    }

    /// A bare ampersand in log output must not eat the text after it.
    #[test]
    fn an_unknown_entity_is_left_alone() {
        assert_eq!(texts("make -j4 & wait"), ["make -j4 & wait"]);
        assert_eq!(texts("PATH=$PATH&stuff;more"), ["PATH=$PATH&stuff;more"]);
    }

    #[test]
    fn a_span_can_wrap_several_lines() {
        let lines = parse("<span style=\"color:#ff0000\">first\nsecond</span>\nthird");
        assert_eq!(lines[0].segments[0].color.as_deref(), Some("#ff0000"));
        assert_eq!(lines[1].segments[0].color.as_deref(), Some("#ff0000"));
        assert_eq!(lines[2].segments[0].color, None);
    }

    #[test]
    fn nested_spans_restore_the_outer_colour() {
        let lines =
            parse("<span style=\"color:#111111\">a<span style=\"color:#222222\">b</span>c</span>");
        let colors: Vec<_> = lines[0]
            .segments
            .iter()
            .map(|s| s.color.as_deref())
            .collect();
        assert_eq!(colors, [Some("#111111"), Some("#222222"), Some("#111111")]);
    }

    /// The colour lands in a `style` attribute, so only hex values are let
    /// through — anything else could smuggle in arbitrary CSS.
    #[test]
    fn non_hex_colours_are_dropped() {
        for tag in [
            "<span style=\"color:red\">x</span>",
            "<span style=\"color:url(http://evil)\">x</span>",
            "<span style=\"color:#12345\">x</span>",
            "<span style=\"color:#gggggg\">x</span>",
            "<span style=\"color:\">x</span>",
        ] {
            assert_eq!(parse(tag)[0].segments[0].color, None, "{tag}");
        }
        assert_eq!(
            parse("<span style=\"color:#abc\">x</span>")[0].segments[0]
                .color
                .as_deref(),
            Some("#abc"),
        );
    }

    #[test]
    fn other_tags_contribute_no_text() {
        assert_eq!(texts("a<br/>b<b>c</b>"), ["ab c".replace(' ', "")]);
    }

    #[test]
    fn carriage_returns_do_not_create_blank_lines() {
        assert_eq!(texts("one\r\ntwo\r\n"), ["one", "two"]);
    }

    #[test]
    fn a_trailing_newline_adds_no_blank_line() {
        assert_eq!(texts("only\n"), ["only"]);
        assert_eq!(texts(""), Vec::<String>::new());
    }

    #[test]
    fn blank_lines_in_the_middle_are_kept() {
        assert_eq!(texts("a\n\nb"), ["a", "", "b"]);
    }

    #[test]
    fn ansi_colours_are_applied_and_reset() {
        let lines = parse("\u{1b}[32mBUILD SUCCESSFUL\u{1b}[0m in 2m");
        let segs = &lines[0].segments;
        assert_eq!(segs[0].text, "BUILD SUCCESSFUL");
        assert_eq!(segs[0].color.as_deref(), Some("#30a46c"));
        assert_eq!(segs[1].text, " in 2m");
        assert_eq!(segs[1].color, None);
    }

    #[test]
    fn ansi_wins_over_an_enclosing_span() {
        let lines = parse("<span style=\"color:#268BD2\">a\u{1b}[31mb\u{1b}[39mc</span>");
        let colors: Vec<_> = lines[0]
            .segments
            .iter()
            .map(|s| s.color.as_deref())
            .collect();
        assert_eq!(colors, [Some("#268BD2"), Some("#e5484d"), Some("#268BD2")]);
    }

    #[test]
    fn extended_ansi_colours_decode() {
        // 256-colour index 196 is a pure red cube entry; truecolor is direct.
        assert_eq!(
            parse("\u{1b}[38;5;196mx")[0].segments[0].color.as_deref(),
            Some("#ff0000")
        );
        assert_eq!(
            parse("\u{1b}[38;2;1;2;3mx")[0].segments[0].color.as_deref(),
            Some("#010203")
        );
    }

    /// Cursor movement, erase-line, OSC titles: all stripped, never shown.
    #[test]
    fn non_colour_escapes_are_stripped() {
        assert_eq!(texts("a\u{1b}[2Kb\u{1b}[1;1Hc"), ["abc"]);
        assert_eq!(texts("x\u{1b}]0;title\u{07}y"), ["xy"]);
        assert_eq!(texts("bold \u{1b}[1mtext\u{1b}[m done"), ["bold text done"]);
    }

    #[test]
    fn to_plain_round_trips_the_whole_body() {
        assert_eq!(
            to_plain("<span style=\"color:#268BD2\">&gt; ls</span>\nfile.txt"),
            "> ls\nfile.txt"
        );
    }
}

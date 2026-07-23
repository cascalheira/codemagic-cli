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
pub fn parse(html: &str) -> Vec<Line> {
    let mut lines = vec![Line::default()];
    // Nested spans are rare but legal; the innermost colour wins, and popping
    // restores the enclosing one.
    let mut colors: Vec<Option<String>> = Vec::new();
    let mut buffer = String::new();
    let mut chars = html.chars().peekable();

    // Flushes `buffer` into the current line, splitting on newlines.
    macro_rules! flush {
        () => {
            if !buffer.is_empty() {
                let color = colors.last().cloned().flatten();
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
        let lines = parse("<span style=\"color:#111111\">a<span style=\"color:#222222\">b</span>c</span>");
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
            parse("<span style=\"color:#abc\">x</span>")[0].segments[0].color.as_deref(),
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
    fn to_plain_round_trips_the_whole_body() {
        assert_eq!(
            to_plain("<span style=\"color:#268BD2\">&gt; ls</span>\nfile.txt"),
            "> ls\nfile.txt"
        );
    }
}

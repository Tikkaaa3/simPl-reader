//! Windows command-line encoding (task T-004 launch semantics).
//!
//! The sampler must launch the target without a shell. `CreateProcessW` gets
//! `lpApplicationName` (the canonical executable path, used verbatim) and one
//! writable wide command line that the target itself parses. This module
//! encodes the argument vector into that command line using the normal
//! Windows quoting rules recognized by `CommandLineToArgvW` and the MSVC C
//! runtime (see the crate README for the citations):
//!
//! - An empty argument becomes `""` and is preserved as an empty argument.
//! - Arguments containing space, tab, or a double quote are wrapped in
//!   double quotes.
//! - Inside a quoted argument, each run of *k* backslashes immediately
//!   followed by a double quote becomes *2k* backslashes plus `\"`; a run of
//!   *k* backslashes not followed by a quote stays *k* backslashes.
//! - Interior NUL values cannot be encoded into a command line and are
//!   rejected; arguments are pre-validated for NUL by `args` too.
//! - Arguments are passed through as wide (UTF-16) bytes; Unicode content
//!   outside the BMP (surrogate pairs) survives untouched.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// Reason an argument vector could not be encoded into a `CreateProcessW`
/// command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLineError {
    /// An argument contains an interior NUL (U+0000), which cannot appear in
    /// a `CreateProcessW` command line.
    EmbeddedNul,
}

/// Encode one argument per the standard Windows quoting rules.
fn encode_argument(arg: &[u16]) -> Vec<u16> {
    let needs_quotes = arg.is_empty()
        || arg
            .iter()
            .any(|&c| c == b' ' as u16 || c == b'\t' as u16 || c == b'"' as u16);
    let mut out = Vec::with_capacity(arg.len() + 2);
    if !needs_quotes {
        out.extend_from_slice(arg);
        return out;
    }
    out.push(b'"' as u16);
    let mut backslashes = 0usize;
    for &c in arg {
        match c {
            0x5C => backslashes += 1,
            0x22 => {
                // k backslashes followed by a quote: emit 2k backslashes
                // plus the escaped quote.
                out.extend(std::iter::repeat_n(0x5C, backslashes * 2));
                out.push(0x5C);
                out.push(0x22);
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n(0x5C, backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    // Trailing backslash run sits before the closing quote: double it.
    out.extend(std::iter::repeat_n(0x5C, backslashes * 2));
    out.push(b'"' as u16);
    out
}

/// Encode the full command line for `argv`, rejecting interior NULs.
/// The result is NOT NUL-terminated; `win` appends the terminator when it
/// hands the buffer to `CreateProcessW`.
pub fn encode_command_line(argv: &[&[u16]]) -> Result<Vec<u16>, CommandLineError> {
    let mut line: Vec<u16> = Vec::new();
    for (i, arg) in argv.iter().enumerate() {
        if arg.contains(&0u16) {
            return Err(CommandLineError::EmbeddedNul);
        }
        if i > 0 {
            out_extend_space(&mut line);
        }
        line.extend(encode_argument(arg));
    }
    Ok(line)
}

fn out_extend_space(line: &mut Vec<u16>) {
    line.push(0x20);
}

/// Convenience wrapper: encode an `OsStr` argument vector (paths included).
pub fn encode_os_command_line(argv: &[&OsStr]) -> Result<Vec<u16>, CommandLineError> {
    let wide: Vec<Vec<u16>> = argv.iter().map(|a| a.encode_wide().collect()).collect();
    let refs: Vec<&[u16]> = wide.iter().map(|v| v.as_slice()).collect();
    encode_command_line(&refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn enc(texts: &[&str]) -> String {
        let wide: Vec<Vec<u16>> = texts.iter().map(|t| s(t)).collect();
        let refs: Vec<&[u16]> = wide.iter().map(|v| v.as_slice()).collect();
        String::from_utf16(&encode_command_line(&refs).unwrap()).unwrap()
    }

    #[test]
    fn plain_argument_is_not_quoted() {
        assert_eq!(enc(&["a"]), "a");
        assert_eq!(enc(&["readme.txt"]), "readme.txt");
    }

    #[test]
    fn empty_argument_is_preserved() {
        assert_eq!(enc(&[""]), "\"\"");
        assert_eq!(enc(&["", "x"]), "\"\" x");
    }

    #[test]
    fn whitespace_argument_is_quoted() {
        assert_eq!(enc(&["a b"]), "\"a b\"");
        let tab: Vec<u16> = vec!['a' as u16, 0x09, 'b' as u16];
        let refs: Vec<&[u16]> = vec![&tab];
        let out = String::from_utf16(&encode_command_line(&refs).unwrap()).unwrap();
        assert_eq!(out, "\"a\tb\"");
    }

    #[test]
    fn embedded_quotes_are_escaped() {
        assert_eq!(enc(&["he said \"hi\""]), "\"he said \\\"hi\\\"\"");
    }

    #[test]
    fn interior_backslashes_unchanged_when_unquoted_context() {
        // a\b has no quoting trigger: stays a\b verbatim.
        assert_eq!(enc(&["a\\b"]), "a\\b");
    }

    #[test]
    fn trailing_backslashes_double_only_before_quotes() {
        // Trailing backslash before the closing quote of a quoted argument
        // doubles.
        assert_eq!(enc(&["a\\ b"]), "\"a\\ b\"");
        // Backslash run immediately before an embedded quote: 2k + \".
        assert_eq!(enc(&["x\\\\\"y"]), "\"x\\\\\\\\\\\"y\"");
    }

    #[test]
    fn unicode_arguments_survive_wide_encoding() {
        let unicode = "café 🙂";
        assert_eq!(enc(&[unicode]), format!("\"{unicode}\""));
        // Surrogate pair beyond BMP survives untouched; no quoting trigger,
        // so it stays unquoted.
        assert_eq!(enc(&["🙂"]), "🙂");
        assert_eq!(enc(&["🙂 x"]), "\"🙂 x\"");
    }

    #[test]
    fn argv0_and_argument_vector_join() {
        let exe = r"C:\Program Files\tool\t.exe";
        assert_eq!(
            enc(&[exe, "arg 1", "", "plain"]),
            format!("\"{exe}\" \"arg 1\" \"\" plain")
        );
    }

    #[test]
    fn embedded_nul_is_rejected() {
        let with_nul: Vec<u16> = vec!['a' as u16, 0, 'b' as u16];
        assert_eq!(
            encode_command_line(&[&with_nul]),
            Err(CommandLineError::EmbeddedNul)
        );
    }

    #[test]
    fn os_str_encoding_matches_wide_slice_encoding() {
        let os: &[&OsStr] = &[OsStr::new("a b"), OsStr::new(""), OsStr::new("é')\\q")];
        let from_os = encode_os_command_line(os).unwrap();
        let wide: Vec<Vec<u16>> = os.iter().map(|a| a.encode_wide().collect()).collect();
        let refs: Vec<&[u16]> = wide.iter().map(|v| v.as_slice()).collect();
        let from_wide = encode_command_line(&refs).unwrap();
        assert_eq!(from_os, from_wide);
    }
}

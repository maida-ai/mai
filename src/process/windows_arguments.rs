//! Windows native executable arguments use CRT quoting. Shell switches remain
//! literal when no quoting is necessary (cmd.exe does not parse quoted /C).
pub(super) fn quote(value: &[u16]) -> Vec<u16> {
    if !value.is_empty() && !value.iter().any(|unit| matches!(unit, 9..=13 | 32 | 34)) {
        return value.to_vec();
    }
    let mut output = vec![34];
    let mut slashes = 0;
    for &unit in value {
        if unit == 92 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            92,
            if unit == 34 { 2 * slashes + 1 } else { slashes },
        ));
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(92, 2 * slashes));
    output.push(34);
    output
}
/// cmd.exe's /S /C protocol is a raw command string, not a CRT argv array.
/// Restrict it to one explicit form instead of silently losing script syntax.
pub(super) fn command_line(
    program: &[u16],
    args: &[Vec<u16>],
    cmd: bool,
) -> anyhow::Result<Vec<u16>> {
    let mut output = quote(program);
    if cmd {
        anyhow::ensure!(
            args.len() == 4
                && args[..3]
                    .iter()
                    .zip(["/D", "/S", "/C"])
                    .all(|(argument, expected)| {
                        argument.iter().copied().eq(expected.encode_utf16())
                            || argument
                                .iter()
                                .copied()
                                .eq(expected.to_ascii_lowercase().encode_utf16())
                    }),
            "cmd.exe requires /D /S /C and exactly one final script argument; pass shell syntax in that single script"
        );
        for argument in &args[..3] {
            output.push(32);
            output.extend(quote(argument));
        }
        output.extend([32, 34]);
        output.extend(&args[3]);
        output.push(34);
    } else {
        for argument in args {
            output.push(32);
            output.extend(quote(argument));
        }
    }
    output.push(0);
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::quote;
    fn encoded(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }
    #[test]
    fn plain_arguments_and_cmd_switches_remain_unquoted() {
        for value in [
            "/D",
            "/S",
            "/C",
            "--exact",
            "native_tree_helper",
            "C:\\tools\\check.exe",
            "hello",
        ] {
            assert_eq!(quote(&encoded(value)), encoded(value), "{value}");
        }
    }
    #[test]
    fn empty_whitespace_quotes_and_backslashes_round_trip_quoting() {
        for (value, expected) in [
            ("", "\"\""),
            ("echo hello", "\"echo hello\""),
            ("a\tb", "\"a\tb\""),
            ("a\"b", "\"a\\\"b\""),
            ("C:\\space path\\", "\"C:\\space path\\\\\""),
        ] {
            assert_eq!(quote(&encoded(value)), encoded(expected), "{value}");
        }
    }
    #[test]
    fn cmd_script_keeps_embedded_quotes_metacharacters_and_native_units() {
        let script = r#"echo "quoted space" & exit /b 7"#;
        let args = ["/D", "/S", "/C", script].map(encoded);
        let actual = super::command_line(&encoded("cmd.exe"), &args, true).unwrap();
        assert_eq!(
            actual,
            encoded("cmd.exe /D /S /C \"echo \"quoted space\" & exit /b 7\"\0")
        );
        let mut native = args.to_vec();
        native[3].push(0xd800);
        let result = super::command_line(&encoded("cmd.exe"), &native, true).unwrap();
        assert_eq!(result[result.len() - 3], 0xd800);
    }
    #[test]
    fn cmd_rejects_ambiguous_argument_arrays() {
        for arguments in [
            vec!["/C", "echo hi"],
            vec!["/D", "/S", "/C", "echo", "hi"],
            vec!["/D", "/S", "/K", "echo hi"],
        ] {
            assert!(
                super::command_line(
                    &encoded("cmd.exe"),
                    &arguments.into_iter().map(encoded).collect::<Vec<_>>(),
                    true
                )
                .is_err()
            );
        }
    }
}

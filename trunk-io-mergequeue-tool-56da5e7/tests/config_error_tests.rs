mod test_utils;
use test_utils::run_mq_with_config;

#[test]
fn test_invalid_enum_value_error_formatting() {
    let config = r#"
[merge]
trigger = "invalid_enum_value"
"#;

    let (exit_code, _stdout, stderr) = run_mq_with_config(config, "config");

    // Should exit with error
    assert_eq!(
        exit_code, 1,
        "Expected exit code 1, got {}. stderr: {}",
        exit_code, stderr
    );

    // Verify error format
    assert!(
        stderr.contains("Invalid configuration in"),
        "stderr should contain 'Invalid configuration in'. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains(".config/mq.toml"),
        "stderr should contain '.config/mq.toml'. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("unknown variant") || stderr.contains("expected one of"),
        "stderr should contain enum error info. stderr: {}",
        stderr
    );

    // Verify it doesn't contain implementation details
    assert!(
        !stderr.contains("confique"),
        "stderr should not contain 'confique'. stderr: {}",
        stderr
    );
}

#[test]
fn test_invalid_mode_enum_error_formatting() {
    let config = r#"
mode = "invalid_mode"
"#;

    let (exit_code, _stdout, stderr) = run_mq_with_config(config, "config");

    assert_eq!(exit_code, 1, "Expected exit code 1. stderr: {}", stderr);
    assert!(
        stderr.contains("Invalid configuration in"),
        "stderr should contain 'Invalid configuration in'. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("unknown variant") || stderr.contains("expected one of"),
        "stderr should contain enum error. stderr: {}",
        stderr
    );
    // The error should mention the valid values
    assert!(
        stderr.contains("singlequeue")
            || stderr.contains("parallelqueue")
            || stderr.contains("expected one of"),
        "stderr should mention valid enum values. stderr: {}",
        stderr
    );
}

#[test]
fn test_invalid_toml_syntax_error_formatting() {
    let config = r#"
[merge]
trigger = "api"
invalid syntax here
"#;

    let (exit_code, _stdout, stderr) = run_mq_with_config(config, "config");

    assert_eq!(exit_code, 1);
    assert!(stderr.contains("Invalid configuration in"));
    // Should show line number for syntax errors
    assert!(stderr.matches(char::is_numeric).count() > 0); // Contains line numbers
}

#[test]
fn test_error_shows_location_format() {
    let config = r#"
[merge]
trigger = "bad_value"
"#;

    let (exit_code, _stdout, stderr) = run_mq_with_config(config, "config");

    assert_eq!(exit_code, 1);

    // Verify location format: filename:line:col
    let lines: Vec<&str> = stderr.lines().collect();
    let location_line = lines
        .iter()
        .find(|l| l.contains("Invalid configuration in"));
    assert!(location_line.is_some());

    let location = location_line.unwrap();
    // Should match pattern like ".config/mq.toml:2:11" or similar
    assert!(location.contains(".config/mq.toml"));
    assert!(location.matches(':').count() >= 2); // filename:line:col format
}

#[test]
fn test_error_message_is_clean() {
    let config = r#"
[merge]
trigger = "invalid"
"#;

    let (exit_code, _stdout, stderr) = run_mq_with_config(config, "config");

    assert_eq!(exit_code, 1);

    // Error message should be clean and readable
    assert!(stderr.contains("unknown variant"));
    assert!(!stderr.contains("confique"));
    assert!(!stderr.contains("Error"));
    assert!(!stderr.contains("from file")); // Should be extracted, not shown in message
}

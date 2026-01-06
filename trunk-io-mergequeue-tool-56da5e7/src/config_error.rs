use regex::Regex;

/// Extract filename from error message
pub fn extract_filename(err_msg: &str) -> Option<String> {
    // Look for patterns like "from file '.config/mq.toml'" or "from file \"mq.toml\""
    let re = Regex::new(r#"from file\s+['"]([^'"]+)['"]"#).ok()?;
    if let Some(caps) = re.captures(err_msg) {
        return Some(caps.get(1)?.as_str().to_string());
    }
    None
}

/// Extract line and column information from error message
pub fn extract_line_info(err_msg: &str) -> Option<(u32, u32)> {
    // Look for patterns like "at line 74, column 11" or "line 74, column 11"
    let re = Regex::new(r"(?:at\s+)?line\s+(\d+)(?:\s*,\s*column\s+(\d+))?").ok()?;

    if let Some(caps) = re.captures(err_msg) {
        let line = caps.get(1)?.as_str().parse().ok()?;
        let col = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        return Some((line, col));
    }

    None
}

/// Format error message for display
pub fn format_config_error(err: &dyn std::error::Error) -> (String, String) {
    // Collect all error messages from the error chain
    let mut error_messages = Vec::new();
    let mut current_err: &dyn std::error::Error = err;
    loop {
        error_messages.push(current_err.to_string());
        if let Some(source) = current_err.source() {
            current_err = source;
        } else {
            break;
        }
    }

    // Combine all error messages for analysis
    let full_error = error_messages.join(": ");

    // Extract filename, line, and column information
    let filename = extract_filename(&full_error);
    let line_info = extract_line_info(&full_error);

    // Extract concise error message (remove file path and line info)
    // First try to find a line without implementation details
    let clean_error = full_error
        .lines()
        .map(|line| line.trim())
        .find(|line| {
            !line.is_empty()
                && line.len() > 1  // Skip single character lines like "|"
                && !line.chars().all(|c| c.is_whitespace() || c == '|' || c == '-' || c == ':' || c == '^')
                && !line.chars().all(|c| c == '|' || c.is_whitespace())  // Skip lines that are just pipes and whitespace
                && !line.contains("confique")
                && !line.contains("Error")
                && !line.starts_with("at")
                && !line.contains("from file")
                && !line.contains(" = ")  // Skip TOML code lines
                && !line.matches(char::is_numeric).count() > 2  // Skip lines that are mostly numbers (like line numbers)
        })
        // If no clean line found, extract the error part from the full error
        .or_else(|| {
            // Try to extract the part after "from file" or after the last colon
            let parts: Vec<&str> = full_error.split(':').collect();
            if parts.len() > 1 {
                // Take the last part which usually contains the actual error
                let last_part = parts.last()?.trim();
                if !last_part.is_empty()
                    && !last_part.contains("confique")
                    && !last_part.contains("Error")
                {
                    return Some(last_part);
                }
            }
            None
        })
        .unwrap_or("invalid configuration");

    // Format location string
    let location = match (filename, line_info) {
        (Some(fname), Some((line, col))) => format!("{}:{}:{}", fname, line, col),
        (Some(fname), None) => fname,
        (None, Some((line, col))) => format!("{}:{}", line, col),
        (None, None) => "unknown location".to_string(),
    };

    (location, clean_error.to_string())
}

/// Handle configuration load errors with helpful error messages
pub fn handle_config_load_error(err: impl std::error::Error) -> ! {
    let (location, clean_error) = format_config_error(&err);
    eprintln!("Invalid configuration in {}", location);
    eprintln!("  {}", clean_error);
    std::process::exit(1);
}

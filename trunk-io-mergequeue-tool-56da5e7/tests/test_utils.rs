use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn get_binary_path() -> PathBuf {
    // In tests, the binary is in target/debug/mq
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("mq");

    // If not found, try deps directory (for test builds)
    if !path.exists() {
        let mut deps_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        deps_path.push("target");
        deps_path.push("debug");
        deps_path.push("deps");
        deps_path.push("mq");
        if deps_path.exists() {
            return deps_path;
        }
    }

    path
}

pub fn run_mq_with_config(config_content: &str, subcommand: &str) -> (i32, String, String) {
    let binary = get_binary_path();

    if !binary.exists() {
        panic!("Binary not found at {:?}. Run 'cargo build' first.", binary);
    }

    // Create a temporary directory for the test with unique name
    let temp_dir = std::env::temp_dir().join(format!(
        "mq_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    // Create .config directory
    let config_dir = temp_dir.join(".config");
    fs::create_dir_all(&config_dir).unwrap();

    // Write the config file
    let config_file = config_dir.join("mq.toml");
    fs::write(&config_file, config_content).unwrap();

    // Run the binary
    let output = Command::new(&binary)
        .arg(subcommand)
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute binary");

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

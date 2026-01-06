use std::process::Command;

fn exec(cmd: &str, args: &[&str]) -> Result<String, String> {
    exec_with_env(cmd, args, None)
}

fn exec_with_env(
    cmd: &str,
    args: &[&str],
    env_vars: Option<&[(&str, &str)]>,
) -> Result<String, String> {
    exec_with_env_quiet(cmd, args, env_vars, false)
}

fn exec_with_env_quiet(
    cmd: &str,
    args: &[&str],
    env_vars: Option<&[(&str, &str)]>,
    quiet: bool,
) -> Result<String, String> {
    let mut command = Command::new(cmd);
    command.args(args);

    if let Some(envs) = env_vars {
        for (key, value) in envs {
            command.env(key, value);
        }
    }

    let output = command
        .output()
        .unwrap_or_else(|_| panic!("Failed to execute {}", cmd));

    if !output.status.success() {
        if !quiet {
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            eprintln!("Call to {} {} failed", cmd, args.join(" "));
        }
        return Err(String::from_utf8_lossy(&output.stderr)
            .into_owned()
            .trim()
            .to_string());
    } else {
        return Ok(String::from_utf8_lossy(&output.stdout)
            .into_owned()
            .trim()
            .to_string());
    }
}

pub fn run_cmd(cmd: &str) -> String {
    let args: Vec<&str> = cmd.split_whitespace().collect();
    exec(args.first().unwrap(), &args[1..]).expect("run failed")
}

pub fn try_gh(args: &[&str], token: &str) -> Result<String, String> {
    exec_with_env("gh", args, Some(&[("GH_TOKEN", token)]))
}

pub fn git(args: &[&str]) -> String {
    exec("git", args).expect("git exec failed")
}

pub fn try_git(args: &[&str]) -> Result<String, String> {
    exec("git", args)
}

pub fn try_git_quiet(args: &[&str]) -> Result<String, String> {
    exec_with_env_quiet("git", args, None, true)
}

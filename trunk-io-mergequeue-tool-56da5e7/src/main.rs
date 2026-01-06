use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, thread};

use chrono::{DateTime, Utc};
use clap::Parser;
use confique::Config;
use gen::cli::{Cli, Subcommands};
use gen::config::{Conf, EnqueueTrigger};
use gen::config_error::handle_config_load_error;
use gen::edit::edit_files_for_pr;
use gen::github::GitHub;
use gen::process::{git, run_cmd, try_gh, try_git, try_git_quiet};
use gen::trunk::{submit_pull_request, upload_targets};
use rand::Rng;
use regex::Regex;
use serde_json::{to_string_pretty, Value};
use walkdir::WalkDir;

/// Get the first GitHub token or exit with an error
fn get_first_github_token(cli: &Cli) -> String {
    let github_tokens = cli.get_github_tokens();
    if github_tokens.is_empty() {
        eprintln!("No GitHub tokens provided. Use --gh-token to specify at least one token.");
        std::process::exit(1);
    }
    github_tokens[0].clone()
}

fn get_txt_files(config: &Conf) -> std::io::Result<Vec<PathBuf>> {
    let mut path = std::env::current_dir()?;
    path.push(&config.pullrequest.change_code_path);
    let mut paths = Vec::new();
    for entry in WalkDir::new(&path) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry.path().extension().and_then(std::ffi::OsStr::to_str) == Some("txt")
        {
            paths.push(entry.path().to_path_buf());
        }
    }
    Ok(paths)
}

fn housekeeping(config: &Conf, gh_token: &str) {
    for _ in 0..3 {
        let json_str = try_gh(
            &[
                "pr",
                "list",
                "--limit=1000",
                "--json",
                "number,mergeable,comments",
            ],
            gh_token,
        )
        .expect("Failed to list PRs");
        let v: Value = serde_json::from_str(&json_str).expect("Failed to parse JSON");

        let mut has_unknown = false;
        let mut requeued: HashSet<String> = HashSet::new();
        if let Some(array) = v.as_array() {
            for item in array {
                let mergeable = item["mergeable"].as_str().unwrap_or("");
                let pr = item["number"].as_i64().unwrap_or(0).to_string();
                match mergeable {
                    "UNKNOWN" => {
                        has_unknown = true;
                    }
                    "CONFLICTING" => {
                        GitHub::close(&pr, gh_token);
                        println!("closed pr: {} (had merge conflicts)", &pr);
                    }
                    "MERGEABLE" => {
                        if requeued.contains(&pr) {
                            continue;
                        }
                        let comments = item["comments"].as_array().unwrap();
                        'comment: for comment in comments.iter() {
                            let body = comment["body"].as_str().unwrap_or("");
                            let created_at_str = comment["createdAt"].as_str().unwrap_or("");

                            if created_at_str.is_empty() {
                                continue 'comment;
                            }

                            let stale_age = Utc::now() - config.close_stale_after_duration();
                            let created_at = DateTime::parse_from_rfc3339(created_at_str)
                                .expect("Unable to parse datetime")
                                .with_timezone(&Utc);

                            if created_at > stale_age {
                                continue 'comment; // The datetime was less than stale age
                            }

                            if config
                                .pullrequest
                                .detect_stale_pr_comments
                                .iter()
                                .any(|s| body.contains(s))
                            {
                                //enqueue(&pr, config, cli, gh_token);
                                GitHub::close(&pr, gh_token);
                                println!("closed stale pr: {}", &pr);
                                requeued.insert(pr.to_owned());
                            }
                        }
                    }
                    _ => {
                        // handle other states
                    }
                }
            }

            if has_unknown {
                thread::sleep(Duration::from_secs(10));
            } else {
                return;
            }
        } else {
            return;
        }
    }
}

fn configure_git(config: &Conf) {
    git(&["config", "user.email", &config.git.email]);
    git(&["config", "user.name", &config.git.name]);
}

fn get_repo_info() -> Result<(String, String), String> {
    let remote_url = git(&["config", "--get", "remote.origin.url"]);
    let re = Regex::new(r"[:/]([^/]+)/([^/]+?)(?:\.git)?$").unwrap();

    if let Some(caps) = re.captures(&remote_url) {
        let owner = caps.get(1).map_or("", |m| m.as_str()).to_string();
        let name = caps.get(2).map_or("", |m| m.as_str()).to_string();
        Ok((owner, name))
    } else {
        Err("Could not parse repository owner and name from remote URL".to_string())
    }
}

fn enqueue(pr: &str, config: &Conf, cli: &Cli, gh_token: &str) {
    match config.merge.trigger {
        EnqueueTrigger::Comment => {
            if config.merge.comment.is_empty() {
                eprintln!("Cannot enqueue PR because merge 'trigger' is set to comment but no comment was provided");
                return;
            }
            GitHub::comment(pr, &config.merge.comment, gh_token);
        }
        EnqueueTrigger::Label => {
            if config.merge.labels.is_empty() {
                eprintln!("Cannot enqueue PR because merge 'trigger' is set to label but no labels were provided");
                return;
            }
            let labels: Vec<&str> = config.merge.labels.split(',').map(|s| s.trim()).collect();
            for lbl in &labels {
                GitHub::add_label(pr, lbl, gh_token);
            }
        }
        EnqueueTrigger::Run => {
            if config.merge.run.is_empty() {
                eprintln!("Cannot enqueue PR because merge 'trigger' is set to run but no run command was provided");
                return;
            }
            // perform token replacement for pr
            let cmd = config.merge.run.replace("{{PR_NUMBER}}", pr);
            println!("run commd {}", cmd);
            let result = run_cmd(&cmd);
            println!("merge run results: {}", result);
        }

        EnqueueTrigger::Api => {
            // TRUNK_TOKEN will be checked at runtime in submit_pull_request
            match get_repo_info() {
                Ok((owner, name)) => {
                    let pr_number: u32 = match pr.parse() {
                        Ok(num) => num,
                        Err(_) => {
                            eprintln!("Invalid PR number: {}", pr);
                            return;
                        }
                    };
                    // Get the PR's base branch
                    let target_branch = GitHub::get_pr_base_branch(pr, gh_token);
                    println!("Enqueuing PR {} targeting branch: {}", pr, target_branch);
                    match submit_pull_request(
                        &owner,
                        &name,
                        pr_number,
                        &target_branch,
                        None, // Default priority, could be made configurable
                        &config.trunk.api,
                        cli,
                    ) {
                        Ok(_) => println!(
                            "Successfully submitted PR {} to Trunk merge queue (target: {})",
                            pr, target_branch
                        ),
                        Err(e) => {
                            eprintln!("Failed to submit PR {} to Trunk merge queue: {}", pr, e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get repository information: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn simulate_test(config: &Conf) -> bool {
    let is_merge_str = env::var("IS_MERGE").unwrap_or_else(|_| String::from("false"));
    let is_merge = is_merge_str.to_lowercase() == "true";

    if !is_merge {
        println!("no flake or sleep when running on pr branch");
        return true;
    }

    println!("sleeping for {} seconds", config.sleep_duration().as_secs());
    thread::sleep(config.sleep_duration());

    if !config.pullrequest.logical_conflict_file.is_empty()
        && Path::new(&config.pullrequest.logical_conflict_file).exists()
    {
        // this pull request is simulating a "logical merge conflict" and should always fail
        println!("Simulating logical merge conflict - failing test");
        return false;
    }

    let mut rng = rand::thread_rng();
    let random_float = rng.gen_range(0.0..1.0);

    println!("Random float: {}", random_float);
    println!("Flake rate: {}", config.test.flake_rate);

    random_float > config.test.flake_rate
}

fn maybe_add_logical_merge_conflict(last_pr: u32, config: &Conf) -> bool {
    if config.pullrequest.logical_conflict_file.is_empty()
        || config.pullrequest.logical_conflict_every == 0
    {
        return false;
    }

    // check if we should simulate a logical merge conflict with this pull request
    if (last_pr + 1) % config.pullrequest.logical_conflict_every != 0 {
        return false;
    }

    println!(
        "logical conflict every {} prs",
        config.pullrequest.logical_conflict_every
    );

    // create logical conflict
    let filename = &config.pullrequest.logical_conflict_file;
    std::fs::write(filename, "simulate logical merge conflict")
        .expect("Unable to write logical merge conflict file");

    git(&["add", &config.pullrequest.logical_conflict_file]);
    true
}

fn checkout_branch(branch: &str) -> Result<(), String> {
    // Check if branch exists locally (quietly - we expect this to fail if branch doesn't exist)
    let branch_exists_locally =
        try_git_quiet(&["rev-parse", "--verify", &format!("refs/heads/{}", branch)]).is_ok();

    // Switch to the branch
    if branch_exists_locally {
        // Branch exists locally, just checkout
        if let Err(e) = try_git(&["checkout", branch]) {
            return Err(format!(
                "Failed to checkout existing branch '{}': {}",
                branch, e
            ));
        }
    } else {
        // Branch doesn't exist locally, fetch from origin to check if it exists there
        let _ = try_git(&["fetch", "origin", branch]);

        // Check if it exists on origin (quietly - we expect this to fail if branch doesn't exist)
        let remote_branch = format!("origin/{}", branch);
        let remote_exists =
            gen::process::try_git_quiet(&["rev-parse", "--verify", &remote_branch]).is_ok();

        if remote_exists {
            // Branch exists on origin, create local tracking branch
            if let Err(e) = try_git(&["checkout", "-b", branch, &remote_branch]) {
                return Err(format!(
                    "Failed to create local branch '{}' tracking {}: {}",
                    branch, remote_branch, e
                ));
            }
        } else {
            return Err(format!(
                "Base branch '{}' does not exist locally or on origin.",
                branch
            ));
        }
    }

    Ok(())
}

fn get_last_pr(gh_token: &str) -> u32 {
    let result = try_gh(&["pr", "list", "--limit=1", "--json", "number"], gh_token);
    if result.is_err() {
        return 0;
    }
    let json_str = result.unwrap();

    let v: Value = serde_json::from_str(&json_str).expect("Failed to parse JSON");

    let array = v.as_array();
    match array {
        Some(pr_array) => {
            if pr_array.is_empty() {
                // No PRs in the system. return 0
                return 0;
            }
            let first = pr_array.first().cloned();
            match first {
                Some(pr) => pr["number"].as_u64().unwrap_or(0) as u32,
                None => 0,
            }
        }
        None => 0,
    }
}

fn create_pull_request(
    filenames: &[String],
    last_pr: u32,
    config: &Conf,
    dry_run: bool,
    gh_token: &str,
    base_branch: &str,
) -> Result<String, String> {
    let lc = maybe_add_logical_merge_conflict(last_pr, config);

    let current_branch = git(&["branch", "--show-current"]);

    // Checkout the base branch (will fetch from origin if needed)
    checkout_branch(base_branch)?;

    // Pull latest changes
    let _ = try_git(&["pull"]);

    // Now edit the files to create changes (after we're on the correct base branch)
    let next_pr_number = last_pr + 1;
    let words = edit_files_for_pr(filenames, next_pr_number, config);

    let branch_name = format!("change/{}", words.join("-"));
    git(&["checkout", "-b", &branch_name]);

    // Stage all changes (including untracked files)
    let _ = try_git(&["add", "-A"]);

    // Check if there are any changes to commit
    let status_output = try_git(&["status", "--porcelain"]);
    if let Ok(output) = status_output {
        if output.trim().is_empty() {
            return Err(format!("No changes to commit for PR. Words: {:?}", words));
        }
    }

    let commit_msg = format!("Moving words {}", words.join(", "));
    if let Err(e) = try_git(&["commit", "--no-verify", "-m", &commit_msg]) {
        return Err(format!("Failed to commit changes: {}", e));
    }

    if !dry_run {
        let result = try_git(&["push", "--set-upstream", "origin", "HEAD"]);
        if result.is_err() {
            git(&["checkout", &current_branch]);
            git(&["pull"]);
            return Err("could not push to origin".to_owned());
        }
    }

    let mut title = format!("[{}] {}", base_branch, words.join(", "));
    if lc {
        title = format!("{} (logical-conflict)", title);
    }

    let mut body = config.pullrequest.body.to_string();
    body.push_str("\n\n[test]\n");
    body.push_str(&format!("flake rate: {}\n", config.test.flake_rate));
    body.push_str(&format!(
        "logical conflict every: {}\n",
        config.pullrequest.logical_conflict_every
    ));
    body.push_str(&format!(
        "sleep for: {}s\n",
        config.sleep_duration().as_secs()
    ));
    body.push_str(&format!(
        "close stale after: {}\n",
        config.pullrequest.close_stale_after
    ));
    body.push_str(&format!(
        "\n\n[pullrequest]\nrequests per hour: {}\ntarget branch: {}\n",
        config.pullrequest.requests_per_hour, base_branch
    ));

    let mut first_letters: Vec<_> = words
        .iter()
        .map(|word| word.chars().next().unwrap().to_string())
        .collect();

    first_letters.sort();
    first_letters.dedup();

    body.push_str(&format!(
        "\n\ndeps=[{}]\n",
        first_letters.into_iter().collect::<Vec<_>>().join(",")
    ));

    let mut args: Vec<&str> = vec![
        "pr",
        "create",
        "--title",
        &title,
        "--body",
        &body,
        "--base",
        base_branch,
    ];

    for lbl in config.pullrequest.labels.split(',') {
        args.push("--label");
        args.push(lbl.trim());
    }

    if dry_run {
        git(&["checkout", &current_branch]);
        git(&["pull"]);
        return Ok((last_pr + 1).to_string());
    }

    let result = try_gh(args.as_slice(), gh_token);

    // no matter what is result - need to reset checkout and clean up
    git(&["checkout", &current_branch]);
    // Clean up any uncommitted changes and untracked files
    let _ = try_git(&["reset", "--hard", "HEAD"]);
    let _ = try_git(&["clean", "-fd"]);
    git(&["pull"]);

    if result.is_err() {
        return Err("could not create pull request".to_owned());
    }

    let pr_url = result.unwrap();
    let re = Regex::new(r"(.*)/pull/(\d+)$").unwrap();
    let caps = re.captures(pr_url.trim()).unwrap();
    let pr_number = caps.get(2).map_or("", |m| m.as_str());

    Ok(pr_number.to_string())
}

fn generate(config: &Conf, cli: &Cli) -> anyhow::Result<()> {
    if config.is_generator_disabled() {
        println!("generator is disabled pull requests per hour is set to 0");
        return Ok(());
    }

    configure_git(config);

    let pull_requests_to_make: usize;
    let pull_request_every: u64;

    if config.pullrequest.requests_per_run > 0 {
        pull_requests_to_make = config.pullrequest.requests_per_run as usize;
        pull_request_every = 1;

        println!(
            "will generate {} requests in burst mode",
            pull_requests_to_make
        );
    } else {
        let dur = config.run_generate_for_duration();
        let hours = dur.as_secs() as f32 / 3600.0;

        pull_requests_to_make =
            (config.pullrequest.requests_per_hour as f32 * hours).ceil() as usize;
        // assuming that generating a pr doesn't take any time we will project to sleep every
        pull_request_every = (dur.as_secs() as f32 / pull_requests_to_make as f32).ceil() as u64;

        println!(
            "will generate pull request every {} seconds",
            pull_request_every
        );
    }

    // get the most recent PR to be created (used for creating logical merge conflicts)
    let github_tokens = cli.get_github_tokens();
    if github_tokens.is_empty() {
        eprintln!("No GitHub tokens provided. Use --gh-token to specify at least one token.");
        std::process::exit(1);
    }
    let mut last_pr = get_last_pr(&github_tokens[0]);

    let mut prs: Vec<String> = Vec::new();

    if cli.dry_run {
        println!("dry-run set - no actual pull requests will be generated");
    }

    if !github_tokens.is_empty() {
        println!(
            "Using {} GitHub token(s) in round-robin fashion",
            github_tokens.len()
        );
    }

    for token_index in 0..pull_requests_to_make {
        let start = Instant::now();
        let files = get_txt_files(config)?;
        let mut filenames: Vec<String> = files
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();

        filenames.sort();

        // Select token for this PR (round-robin)
        let current_token = &github_tokens[token_index % github_tokens.len()];

        // Select base branch for this PR (round-robin from protected_branches)
        let protected_branches = &config.pullrequest.protected_branches;
        let base_branch = &protected_branches[token_index % protected_branches.len()];

        let pr_result = create_pull_request(
            &filenames,
            last_pr,
            config,
            cli.dry_run,
            current_token,
            base_branch,
        );
        if pr_result.is_err() {
            println!("problem created pr for files: {:?}", filenames);
            continue;
        }
        let duration = start.elapsed();
        let pr = pr_result.unwrap();
        println!(
            "created pr: {} (target: {}) in {:?} // waiting: {} mins",
            pr,
            base_branch,
            duration,
            (pull_request_every as f32 / 60.0)
        );
        thread::sleep(Duration::from_secs(pull_request_every) / 2);
        enqueue(&pr, config, cli, current_token); // Change the argument type to accept a String
        thread::sleep(Duration::from_secs(pull_request_every) / 2);
        prs.push(pr);
        last_pr += 1;
    }

    Ok(())
}

fn run() -> anyhow::Result<()> {
    let cli: Cli = Cli::parse();

    if cli.subcommand.is_none() {
        println!("Subcommand required. run 'mq help'");
        return Ok(());
    }

    if let Some(Subcommands::Defaultconfig {}) = &cli.subcommand {
        Conf::print_default();
        return Ok(());
    }

    let config = Conf::builder()
        .env()
        .file("mq.toml")
        .file(".config/mq.toml")
        .load()
        .unwrap_or_else(|err| handle_config_load_error(err));

    config.is_valid(Some(&cli)).unwrap_or_else(|err| {
        eprintln!("Invalid config:\n    {}", err);
        std::process::exit(1);
    });

    match &cli.subcommand {
        Some(Subcommands::Housekeeping {}) => {
            let token: String = get_first_github_token(&cli);
            housekeeping(&config, &token);
            Ok(())
        }
        Some(Subcommands::TestSim {}) => {
            if !simulate_test(&config) {
                std::process::exit(1);
            }
            Ok(())
        }
        Some(Subcommands::Config {}) => {
            let config_json =
                to_string_pretty(&config).expect("Failed to serialize config to JSON");
            println!("{}", config_json);
            Ok(())
        }
        Some(Subcommands::Generate {}) => generate(&config, &cli),
        Some(Subcommands::UploadTargets(ut)) => {
            // upload_targets(&cli, &gen::pullrequest::get_json()); // &ut.github_json);
            upload_targets(&config, &cli, &ut.github_json);
            Ok(())
        }
        Some(Subcommands::Enqueue(enqueue_args)) => {
            println!("Enqueuing PR: {}", enqueue_args.pr);
            let token = get_first_github_token(&cli);
            enqueue(&enqueue_args.pr, &config, &cli, &token);
            Ok(())
        }
        _ => {
            // Handle other cases here
            Err(anyhow::anyhow!("Subcommand not supported"))
        }
    }
}

fn main() {
    env_logger::init();

    match run() {
        Ok(_) => (),
        Err(err) => {
            log::error!("{}", err);
            std::process::exit(1);
        }
    }
}

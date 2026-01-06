use gen::config::{Conf, MergeConf, PullRequestConf, TestConf};

/// Helper function to create a test config with valid defaults
fn create_test_config(pullrequest: PullRequestConf) -> Conf {
    Conf {
        pullrequest,
        test: TestConf {
            flake_rate: 0.1,                   // Valid flake rate
            sleep_for: "1 second".to_string(), // Valid duration
            ..Default::default()
        },
        merge: MergeConf {
            comment: "test comment".to_string(), // Valid comment for merge trigger
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn test_deterministic_shuffle() {
    let config = create_test_config(PullRequestConf {
        deps_distribution: Some("0.5x1,0.3x2,0.2x3".to_string()),
        ..Default::default()
    });

    // Test that the shuffle is deterministic by running it multiple times
    let mut sequence1 = vec![1, 1, 1, 2, 2, 3, 3, 3, 3, 3];
    let mut sequence2 = vec![1, 1, 1, 2, 2, 3, 3, 3, 3, 3];

    config.deterministic_shuffle(&mut sequence1);
    config.deterministic_shuffle(&mut sequence2);

    // The shuffled sequences should be identical
    assert_eq!(sequence1, sequence2);

    // Verify the shuffle actually changed the order (not just identity)
    let original = vec![1, 1, 1, 2, 2, 3, 3, 3, 3, 3];
    // The shuffle might not change the order for small sequences, so let's test with a larger one
    let mut large_sequence = vec![
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    let large_original = large_sequence.clone();
    config.deterministic_shuffle(&mut large_sequence);
    assert_ne!(large_sequence, large_original);

    // Verify the shuffle preserves all elements (just reorders them)
    let mut sorted_original = original.clone();
    let mut sorted_shuffled = sequence1.clone();
    sorted_original.sort();
    sorted_shuffled.sort();
    assert_eq!(sorted_original, sorted_shuffled);
}

#[test]
fn test_deps_distribution_uniform() {
    let config = create_test_config(PullRequestConf {
        deps_distribution: Some("0.5x1,0.3x2,0.2x3".to_string()),
        ..Default::default()
    });

    // Test that the same PR number always returns the same dependency count
    let pr_1_count_1 = config.get_dependency_count(1, 10);
    let pr_1_count_2 = config.get_dependency_count(1, 10);
    assert_eq!(pr_1_count_1, pr_1_count_2);

    let pr_100_count_1 = config.get_dependency_count(100, 10);
    let pr_100_count_2 = config.get_dependency_count(100, 10);
    assert_eq!(pr_100_count_1, pr_100_count_2);

    // Test that different PR numbers can have different counts
    let _pr_1_count = config.get_dependency_count(1, 10);
    let _pr_2_count = config.get_dependency_count(2, 10);
    // They might be the same or different, but the important thing is consistency

    // Test wrapping around (PR 1001 should be same as PR 1)
    let pr_1_count = config.get_dependency_count(1, 10);
    let pr_1001_count = config.get_dependency_count(1001, 10);
    assert_eq!(pr_1_count, pr_1001_count);

    // Test distribution over a range
    let mut counts = Vec::new();
    for pr_num in 1..=100 {
        counts.push(config.get_dependency_count(pr_num, 10));
    }

    // Count occurrences of each dependency count
    let mut count_1 = 0;
    let mut count_2 = 0;
    let mut count_3 = 0;

    for count in &counts {
        match count {
            1 => count_1 += 1,
            2 => count_2 += 1,
            3 => count_3 += 1,
            _ => panic!("Unexpected dependency count: {}", count),
        }
    }

    // With 50%x1, 30%x2, 20%x3 over 100 PRs, we should get roughly:
    // ~50 PRs with 1 dependency, ~30 with 2, ~20 with 3
    // Allow more tolerance for rounding and distribution variations
    assert!(
        count_1 >= 40 && count_1 <= 60,
        "Expected ~50 PRs with 1 dependency, got {}",
        count_1
    );
    assert!(
        count_2 >= 20 && count_2 <= 40,
        "Expected ~30 PRs with 2 dependencies, got {}",
        count_2
    );
    assert!(
        count_3 >= 10 && count_3 <= 30,
        "Expected ~20 PRs with 3 dependencies, got {}",
        count_3
    );

    // Ensure we have some of each type (distribution is working)
    assert!(
        count_1 > 0,
        "Should have at least some PRs with 1 dependency"
    );
    assert!(
        count_2 > 0,
        "Should have at least some PRs with 2 dependencies"
    );
    assert!(
        count_3 > 0,
        "Should have at least some PRs with 3 dependencies"
    );
}

#[test]
fn test_fallback_to_old_behavior() {
    let config = create_test_config(PullRequestConf {
        max_deps: 5,
        max_impacted_deps: 3,
        deps_distribution: None, // Use old behavior
        ..Default::default()
    });

    // Should fall back to old behavior: min(max_deps, max_impacted_deps, total_available)
    let count = config.get_dependency_count(1, 10);
    assert_eq!(count, 3); // min(5, 3, 10) = 3

    let count = config.get_dependency_count(1, 2);
    assert_eq!(count, 2); // min(5, 3, 2) = 2
}

#[test]
fn test_edit_files_for_pr_logic() {
    // Test the logic of edit_files_for_pr without requiring actual files
    // This tests the file selection and count logic

    let config_new = create_test_config(PullRequestConf {
        max_deps: 3,
        max_impacted_deps: 2,
        deps_distribution: Some("0.5x1,0.5x2".to_string()),
        ..Default::default()
    });

    let config_old = create_test_config(PullRequestConf {
        max_deps: 2,
        max_impacted_deps: 1,
        deps_distribution: None, // Use old behavior
        ..Default::default()
    });

    // Test the dependency count logic directly
    let filenames = vec![
        "file1.txt".to_string(),
        "file2.txt".to_string(),
        "file3.txt".to_string(),
    ];

    // Test new distribution approach
    let dependency_count_new = config_new.get_dependency_count(1, filenames.len());
    assert!(dependency_count_new >= 1 && dependency_count_new <= 2); // Should be 1 or 2 based on distribution

    // Test old approach
    let dependency_count_old = config_old.get_dependency_count(1, filenames.len());
    assert_eq!(dependency_count_old, 1); // min(max_deps=2, max_impacted_deps=1, total_available=3) = 1

    // Test deterministic behavior
    let count1 = config_new.get_dependency_count(1, filenames.len());
    let count2 = config_new.get_dependency_count(1, filenames.len());
    assert_eq!(count1, count2);
}

#[test]
fn test_edit_files_for_pr_with_real_files() {
    use gen::edit::edit_files_for_pr;
    use std::fs;

    // Create a temporary directory for test files
    let temp_dir = std::env::temp_dir().join("mq_test_files");
    fs::create_dir_all(&temp_dir).unwrap();

    // Create test files with content
    let file1_path = temp_dir.join("file1.txt");
    let file2_path = temp_dir.join("file2.txt");
    let file3_path = temp_dir.join("file3.txt");

    fs::write(&file1_path, "alpha\nbeta\ngamma\n").unwrap();
    fs::write(&file2_path, "delta\nepsilon\nzeta\n").unwrap();
    fs::write(&file3_path, "eta\ntheta\niota\n").unwrap();

    let config = create_test_config(PullRequestConf {
        max_deps: 3,
        max_impacted_deps: 2,
        deps_distribution: Some("1.0x2".to_string()), // Always use 2 dependencies
        ..Default::default()
    });

    let filenames = vec![
        file1_path.to_string_lossy().to_string(),
        file2_path.to_string_lossy().to_string(),
        file3_path.to_string_lossy().to_string(),
    ];

    // Test the actual file editing
    let words = edit_files_for_pr(&filenames, 1, &config);

    // Debug: Check what dependency count we actually got
    let dependency_count = config.get_dependency_count(1, filenames.len());
    println!("Expected 2 dependencies, got {} for PR 1", dependency_count);
    println!("Distribution: {:?}", config.pullrequest.deps_distribution);

    // Should return 2 words (based on distribution "1.0x2")
    assert_eq!(words.len(), 2);

    // Words should be lowercase versions of the original words
    for word in &words {
        assert!(word.chars().all(|c| c.is_lowercase()));
    }

    // Clean up test files
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_deps_distribution_validation() {
    // Test valid distributions
    let valid_configs = vec![
        "1.0x1",
        "0.5x1,0.5x2",
        "0.75x1,0.15x2,0.09x3,0.01xALL",
        "0.8x1,0.2xALL",
    ];

    for distribution in valid_configs {
        let config = create_test_config(PullRequestConf {
            deps_distribution: Some(distribution.to_string()),
            ..Default::default()
        });

        let result = config.is_valid(None);
        if let Err(e) = result {
            panic!(
                "Valid distribution '{}' should pass validation, but got error: {}",
                distribution, e
            );
        }
    }

    // Test invalid distributions
    let invalid_configs = vec![
        ("", "empty string"),
        ("0.5x1,0.3x2", "probabilities don't sum to 1.0"),
        ("1.5x1", "probability > 1.0"),
        ("0.5x1,0.5x0", "count is 0"),
        ("0.5x1,0.5x-1", "negative count"),
        ("0.5x1,0.5", "missing 'x' separator"),
        ("0.5x1,0.5x", "missing count after 'x'"),
        ("x1", "missing probability before 'x'"),
        ("0.5x1,0.5xINVALID", "invalid count format"),
    ];

    for (distribution, description) in invalid_configs {
        let config = create_test_config(PullRequestConf {
            deps_distribution: Some(distribution.to_string()),
            ..Default::default()
        });

        assert!(
            config.is_valid(None).is_err(),
            "Invalid distribution '{}' ({}) should fail validation",
            distribution,
            description
        );
    }
}

#[test]
fn test_validate_deps_distribution_directly() {
    // Test valid distributions directly
    let valid_distributions = vec!["1.0x1", "0.5x1,0.5x2", "0.75x1,0.15x2,0.09x3,0.01xALL"];

    for distribution in valid_distributions {
        // Create a config with just the dependency distribution set
        let config = create_test_config(PullRequestConf {
            deps_distribution: Some(distribution.to_string()),
            ..Default::default()
        });

        let result = config.is_valid(None);
        assert!(
            result.is_ok(),
            "Valid distribution '{}' should pass validation, but got error: {:?}",
            distribution,
            result
        );
    }
}

#[test]
fn test_simple_distribution() {
    let config = create_test_config(PullRequestConf {
        deps_distribution: Some("1.0x2".to_string()),
        ..Default::default()
    });

    // Test that "1.0x2" always returns 2
    for pr_num in 1..=10 {
        let count = config.get_dependency_count(pr_num, 10);
        assert_eq!(count, 2, "PR {} should have 2 dependencies", pr_num);
    }
}

#[test]
fn test_api_trigger_trunk_token_validation_in_config() {
    use gen::config::EnqueueTrigger;

    // Test with API trigger - should pass config validation
    // TRUNK_TOKEN validation is now done at runtime when actually needed,
    // not during config validation
    let mut config = create_test_config(PullRequestConf::default());
    config.merge.trigger = EnqueueTrigger::Api;

    // Config validation should pass regardless of token presence
    // Token validation happens at runtime in submit_pull_request/upload_targets
    let result = config.is_valid(None);
    assert!(
        result.is_ok(),
        "API trigger should pass config validation - token check is at runtime"
    );
}

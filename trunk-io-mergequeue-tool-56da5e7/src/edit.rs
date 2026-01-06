use crate::config::Conf;
use rand::Rng;
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::io::{BufReader, BufWriter};

pub fn change_file(filenames: &[String], count: u32) -> Vec<String> {
    let mut rng = rand::thread_rng();
    let mut words: Vec<String> = Vec::new();

    if count > filenames.len() as u32 {
        panic!("The count must be less than the number of files");
    }

    // Create a vector of indices and shuffle it to get unique random selections
    let mut indices: Vec<usize> = (0..filenames.len()).collect();
    for i in 0..count as usize {
        let j = rng.gen_range(i..indices.len());
        indices.swap(i, j);
    }

    // Take the first 'count' indices and process those files
    for i in 0..count as usize {
        let filename = &filenames[indices[i]];
        words.push(move_random_line(filename));
    }

    words
}

pub fn move_random_line(filename: &str) -> String {
    // Read the file into a vector of lines
    let file = std::fs::File::open(&filename).expect("Failed to open file");
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader
        .lines()
        .collect::<Result<_, _>>()
        .expect("failed to read lines");

    if lines.is_empty() {
        panic!("Cannot continue the file {} is empty", filename);
    }

    // Choose a random line
    let mut rng = rand::thread_rng();
    let line_index = rng.gen_range(0..lines.len());

    // Remove the line from the vector
    let line = lines.remove(line_index);
    let word = line.trim().to_string();

    // Choose another random line
    let other_line_index = rng.gen_range(0..lines.len());

    // Insert the line at the new position
    lines.insert(other_line_index, line);

    // Write the lines back to the file
    let file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&filename)
        .expect("failed to open file");
    let mut writer = BufWriter::new(file);
    for line in lines {
        writeln!(writer, "{}", line).expect("failed to write file");
    }

    word.to_lowercase().to_string()
}

/// Edit files for a PR based on the configuration and PR number.
/// Returns the words that were changed in the files.
pub fn edit_files_for_pr(filenames: &[String], pr_number: u32, config: &Conf) -> Vec<String> {
    // Check if using new distribution approach or old approach
    let (selected_files, change_count) = if config.pullrequest.deps_distribution.is_some() {
        // New approach: use all available files, change dependency_count lines
        let dependency_count = config.get_dependency_count(pr_number, filenames.len());
        (filenames.to_vec(), dependency_count)
    } else {
        // Old approach: limit files to max_deps, change max_impacted_deps lines
        let max_files = config.pullrequest.max_deps.min(filenames.len());
        let files: Vec<String> = filenames.iter().take(max_files).cloned().collect();
        (files, config.pullrequest.max_impacted_deps)
    };

    change_file(&selected_files, change_count as u32)
}

use rayon::prelude::*;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct DeterministicSearch {
    max_results: Option<usize>,
}

impl DeterministicSearch {
    pub fn new(max_results: Option<usize>) -> Self {
        Self { max_results }
    }

    /// Searches through the provided files using the given search function.
    /// Logs progress every `log_every` files processed.
    pub fn search_with_info<F, T, I>(
        &self,
        files: &HashMap<PathBuf, I>,
        search_fn: F,
        log_every: usize, // Add a parameter to specify logging interval
    ) -> Vec<T>
    where
        F: Fn(&PathBuf, &I) -> Option<T> + Send + Sync,
        T: Send,
        I: Send + Sync,
    {
        // Sort files for deterministic ordering
        let mut sorted_files: Vec<_> = files.iter().collect();
        sorted_files.sort_by(|(a, _), (b, _)| a.cmp(b));

        let target_count = self.max_results.unwrap_or(usize::MAX);
        let mut results = Vec::new();

        // Initialize an atomic counter wrapped in Arc for shared ownership across threads
        let counter = Arc::new(AtomicUsize::new(0));

        // Process chunks in parallel, but collect results sequentially
        for chunk in sorted_files.chunks(100) {
            // Skip processing if we have enough results
            if results.len() >= target_count {
                break;
            }

            // Clone the Arc to pass into the parallel iterator
            let counter_clone = Arc::clone(&counter);
            let log_every_clone = log_every;

            // Process current chunk in parallel
            let chunk_results: Vec<_> = chunk
                .par_iter()
                .filter_map(|(path, info)| {
                    // Apply the search function
                    let result = search_fn(path, info);

                    // Increment the counter
                    let current = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;

                    // If the counter reaches the logging interval, log the progress
                    if current % log_every_clone == 0 {
                        println!("Processed {} files, current file: {:?}", current, path);
                    }

                    result
                })
                .collect();

            // Add results sequentially until we hit our target
            for result in chunk_results {
                results.push(result);
                if results.len() >= target_count {
                    break;
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "{}", content).unwrap();
        path
    }

    fn find_matches(path: &PathBuf, _info: &i32) -> Option<Vec<String>> {
        let content = fs::read_to_string(path).unwrap();
        if content.trim() == "match" {
            Some(vec![String::from("match")])
        } else {
            None
        }
    }

    #[test]
    fn test_deterministic_results() {
        let temp_dir = TempDir::new().unwrap();
        let mut files = HashMap::new();

        // Create test files with different content to ensure some matches
        for i in 1..=20 {
            let content = if i % 3 == 0 { "match" } else { "no match" };
            let path = create_test_file(&temp_dir, &format!("{:02}.txt", i), content);
            files.insert(path, i);
        }

        let searcher = DeterministicSearch::new(Some(4));

        // Run search multiple times
        let results1 = searcher.search_with_info(&files, find_matches);
        let results2 = searcher.search_with_info(&files, find_matches);

        // Should get exactly 4 results each time
        assert_eq!(results1.len(), 4);
        assert_eq!(results2.len(), 4);

        // Results should be identical
        assert_eq!(results1, results2);

        // Each result should be a vector containing "match"
        for result in &results1 {
            assert_eq!(result, &vec![String::from("match")]);
        }
    }

    #[test]
    fn test_less_matches_than_requested() {
        let temp_dir = TempDir::new().unwrap();
        let mut files = HashMap::new();

        // Create 5 files with only 2 matches
        for i in 1..=5 {
            let content = if i <= 2 { "match" } else { "no match" };
            let path = create_test_file(&temp_dir, &format!("file{}.txt", i), content);
            files.insert(path, i);
        }

        let searcher = DeterministicSearch::new(Some(4));
        let results = searcher.search_with_info(&files, find_matches);

        // Should only get 2 results even though we asked for 4
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_no_limit() {
        let temp_dir = TempDir::new().unwrap();
        let mut files = HashMap::new();

        // Create 10 files, all matches
        for i in 1..=10 {
            let path = create_test_file(&temp_dir, &format!("file{}.txt", i), "match");
            files.insert(path, i);
        }

        let searcher = DeterministicSearch::new(None);
        let results = searcher.search_with_info(&files, find_matches);

        // Should get all 10 matches
        assert_eq!(results.len(), 10);
    }
}

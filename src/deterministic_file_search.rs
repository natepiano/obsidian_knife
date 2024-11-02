use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

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

        // Process chunks in parallel, but collect results sequentially
        for chunk in sorted_files.chunks(100) {
            // Skip processing if we have enough results
            if results.len() >= target_count {
                break;
            }

            // Process current chunk in parallel
            let chunk_results: Vec<_> = chunk
                .par_iter()
                .filter_map(|(path, info)| {
                    // Apply the search function
                    let result = search_fn(path, info);

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
    use tempfile::TempDir;

    #[test]
    fn test_deterministic_results() {
        let searcher = DeterministicSearch::new(None);

        // Create test files map
        let mut files = HashMap::new();
        files.insert(PathBuf::from("file1.md"), "content1");
        files.insert(PathBuf::from("file2.md"), "content2");
        files.insert(PathBuf::from("file3.md"), "content3");

        // Define a simple find matches function with the correct signature
        let find_matches = |path: &PathBuf, _content: &&str| -> Option<Vec<String>> {
            Some(vec![path.to_string_lossy().to_string()])
        };

        // Run multiple searches and verify they return the same results
        let results1 = searcher.search_with_info(&files, find_matches);
        let results2 = searcher.search_with_info(&files, find_matches);

        assert_eq!(results1.len(), results2.len());

        let results1_sorted: Vec<_> = results1.iter().collect();
        let results2_sorted: Vec<_> = results2.iter().collect();

        assert_eq!(
            results1_sorted, results2_sorted,
            "Results should be identical"
        );
    }

    #[test]
    fn test_less_matches_than_requested() {
        let searcher = DeterministicSearch::new(Some(10));
        let temp_dir = TempDir::new().unwrap();

        // Create a HashMap of files with sample content
        let mut files = HashMap::new();

        // Create 5 files
        for i in 1..=5 {
            let path = temp_dir.path().join(format!("file{}.txt", i));
            files.insert(path, format!("content {}", i));
        }

        // Create a match finder that returns a vec of matches for testing
        let find_matches = |path: &PathBuf, _content: &String| -> Option<Vec<String>> {
            Some(vec![format!(
                "match in {}",
                path.file_name().unwrap().to_string_lossy()
            )])
        };

        // Should return all matches since we have fewer files than requested
        let results = searcher.search_with_info(&files, find_matches);
        assert_eq!(results.len(), 5, "Should return matches from all files");

        // Verify each file produced a match
        for i in 1..=5 {
            let expected_match = format!("match in file{}.txt", i);
            assert!(
                results
                    .iter()
                    .any(|matches| matches.contains(&expected_match)),
                "Missing match for file{}.txt",
                i
            );
        }
    }

    #[test]
    fn test_no_limit() {
        let searcher = DeterministicSearch::new(None);
        let mut files = HashMap::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..100 {
            files.insert(temp_dir.path().join(format!("file{}.txt", i)), ());
        }

        // Define match function that matches file50.txt
        let find_matches =
            |file_path: &PathBuf, _: &()| Some(vec![file_path.to_string_lossy().to_string()]);

        let results = searcher.search_with_info(&files, find_matches);
        assert!(!results.is_empty());

        // Convert both strings to same type for comparison
        let found_file50 = results.iter().any(|result_vec| {
            result_vec
                .iter()
                .any(|path_str| path_str.contains(&"file50".to_string()))
        });

        assert!(found_file50, "Should have found file50.txt");
    }

    #[test]
    fn test_with_limit() {
        let searcher = DeterministicSearch::new(Some(5));
        let mut files = HashMap::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..10 {
            files.insert(temp_dir.path().join(format!("file{}.txt", i)), ());
        }

        let find_matches =
            |file_path: &PathBuf, _: &()| Some(vec![file_path.to_string_lossy().to_string()]);

        let results = searcher.search_with_info(&files, find_matches);
        assert_eq!(
            results.len(),
            5,
            "Should only return 5 results due to limit"
        );
    }

    #[test]
    fn test_empty_files() {
        let searcher = DeterministicSearch::new(None);
        let files = HashMap::new();

        let find_matches = |_: &PathBuf, _: &()| Some(vec!["test".to_string()]);

        let results = searcher.search_with_info(&files, find_matches);
        assert!(
            results.is_empty(),
            "Should return no results for empty files"
        );
    }

    #[test]
    fn test_no_matches() {
        let searcher = DeterministicSearch::new(None);
        let mut files = HashMap::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..5 {
            files.insert(temp_dir.path().join(format!("file{}.txt", i)), ());
        }

        let find_matches = |_: &PathBuf, _: &()| -> Option<Vec<String>> { None };

        let results = searcher.search_with_info(&files, find_matches);
        assert!(
            results.is_empty(),
            "Should return no results when no matches found"
        );
    }
}

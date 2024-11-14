use rayon::prelude::*;

pub struct DeterministicSearch {
    max_results: Option<usize>,
}

impl DeterministicSearch {
    pub fn new(max_results: Option<usize>) -> Self {
        Self { max_results }
    }

    /// Searches through the provided files using the given search function.
    /// Logs progress every `log_every` files processed.
    pub fn search_with_info<F, T, I>(&self, files: &[I], search_fn: F) -> Vec<T>
    where
        F: Fn(&I) -> Option<T> + Send + Sync,
        T: Send,
        I: Send + Sync,
    {
        let target_count = self.max_results.unwrap_or(usize::MAX);
        let mut results = Vec::new();

        for chunk in files.chunks(100) {
            if results.len() >= target_count {
                break;
            }

            let chunk_results: Vec<_> = chunk
                .par_iter()
                .filter_map(|info| search_fn(info))
                .collect();

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
    use std::path::PathBuf;

    #[test]
    fn test_deterministic_results() {
        let searcher = DeterministicSearch::new(None);

        // Create test files map
        let files = vec![
            PathBuf::from("file1.md"),
            PathBuf::from("file2.md"),
            PathBuf::from("file3.md"),
        ];

        // Define a simple find matches function with the correct signature
        let find_matches = |path: &PathBuf| -> Option<Vec<String>> {
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

        // Create a Vec of files with sample content
        let mut files = Vec::new();

        // Create 5 files
        for i in 1..=5 {
            let path = temp_dir.path().join(format!("file{}.txt", i));
            files.push(path);
        }

        // Create a match finder that returns a vec of matches for testing
        let find_matches = |path: &PathBuf| -> Option<Vec<String>> {
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
        let mut files = Vec::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..100 {
            files.push(temp_dir.path().join(format!("file{}.txt", i)));
        }

        // Define match function that matches file50.txt
        let find_matches =
            |file_path: &PathBuf| Some(vec![file_path.to_string_lossy().to_string()]);

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
        let mut files = Vec::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..10 {
            files.push(temp_dir.path().join(format!("file{}.txt", i)));
        }

        let find_matches =
            |file_path: &PathBuf| Some(vec![file_path.to_string_lossy().to_string()]);

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
        let files = Vec::new();

        let find_matches = |_: &PathBuf| Some(vec!["test".to_string()]);

        let results = searcher.search_with_info(&files, find_matches);
        assert!(
            results.is_empty(),
            "Should return no results for empty files"
        );
    }

    #[test]
    fn test_no_matches() {
        let searcher = DeterministicSearch::new(None);
        let mut files = Vec::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..5 {
            files.push(temp_dir.path().join(format!("file{}.txt", i)));
        }

        let find_matches = |_: &PathBuf| -> Option<Vec<String>> { None };

        let results = searcher.search_with_info(&files, find_matches);
        assert!(
            results.is_empty(),
            "Should return no results when no matches found"
        );
    }
}

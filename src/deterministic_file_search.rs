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
    pub fn search_with_info<F, T, I>(&self, files: &mut [I], search_fn: F) -> Vec<T>
    where
        F: Fn(&mut I) -> Option<T> + Send + Sync,
        T: Send,
        I: Send + Sync,
    {
        let target_count = self.max_results.unwrap_or(usize::MAX);
        let mut results = Vec::new();

        for chunk in files.chunks_mut(100) {
            if results.len() >= target_count {
                break;
            }

            let chunk_results: Vec<_> = chunk
                .par_iter_mut()
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
    use crate::test_utils::assert_test_case;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct SearchTestCase {
        name: &'static str,
        max_results: Option<usize>,
        file_count: usize,
        find_matches: Box<dyn Fn(&mut PathBuf) -> Option<Vec<String>> + Send + Sync>,
        expected: Vec<Vec<String>>,
    }

    fn setup_test_files(count: usize) -> (TempDir, Vec<PathBuf>) {
        let temp_dir = TempDir::new().unwrap();
        let files: Vec<PathBuf> = (0..count)
            .map(|i| temp_dir.path().join(format!("file{}.txt", i)))
            .collect();
        (temp_dir, files)
    }

    #[test]
    fn test_search_cases() {
        let test_cases = vec![
            SearchTestCase {
                name: "less matches than requested - content check",
                max_results: Some(10),
                file_count: 5,
                find_matches: Box::new(|path| {
                    Some(vec![format!(
                        "match in {}",
                        path.file_name().unwrap().to_string_lossy()
                    )])
                }),
                expected: (0..5)
                    .map(|i| vec![format!("match in file{}.txt", i)])
                    .collect(),
            },
            SearchTestCase {
                name: "with limit",
                max_results: Some(5),
                file_count: 10,
                find_matches: Box::new(|_| Some(vec!["count".to_string()])),
                expected: vec![vec!["count".to_string()]; 5],
            },
            SearchTestCase {
                name: "empty files",
                max_results: None,
                file_count: 0,
                find_matches: Box::new(|_| Some(vec!["test".to_string()])),
                expected: vec![],
            },
            SearchTestCase {
                name: "no matches - should return empty results",
                max_results: None,
                file_count: 5,
                find_matches: Box::new(|_| None),
                expected: vec![],
            },
            SearchTestCase {
                name: "no limit",
                max_results: None,
                file_count: 100,
                find_matches: Box::new(|path| {
                    Some(vec![path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()])
                }),
                expected: (0..100).map(|i| vec![format!("file{}.txt", i)]).collect(),
            },
        ];

        for case in test_cases {
            let searcher = DeterministicSearch::new(case.max_results);
            let (_temp_dir, mut files) = setup_test_files(case.file_count);
            let actual = searcher.search_with_info(&mut files, &case.find_matches);

            assert_test_case(actual, case.expected, case.name, |a, e| {
                assert!(a == e, "Results don't match")
            });
        }
    }

    #[test]
    fn test_deterministic_results() {
        let searcher = DeterministicSearch::new(None);
        let mut files = vec![
            PathBuf::from("file1.md"),
            PathBuf::from("file2.md"),
            PathBuf::from("file3.md"),
        ];
        let find_matches = |path: &mut PathBuf| Some(vec![path.to_string_lossy().to_string()]);

        let results1 = searcher.search_with_info(&mut files, find_matches);
        let results2 = searcher.search_with_info(&mut files, find_matches);

        assert_test_case(
            results1.iter().collect::<Vec<_>>(),
            results2.iter().collect::<Vec<_>>(),
            "deterministic results",
            |a, e| assert!(a == e, "Results don't match"),
        );
    }
}

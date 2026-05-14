use super::constants::FENCED_CODE_DELIMITER;
use super::constants::INLINE_CODE_DELIMITER;

#[derive(Debug, PartialEq)]
enum CodeBlockDelimiter {
    Backtick,
    TripleBacktick,
}

impl TryFrom<&str> for CodeBlockDelimiter {
    type Error = (); // Using unit type for error since we don't care if it fails

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.trim().starts_with(FENCED_CODE_DELIMITER) {
            Ok(Self::TripleBacktick)
        } else {
            Err(())
        }
    }
}

impl TryFrom<char> for CodeBlockDelimiter {
    type Error = ();

    fn try_from(c: char) -> Result<Self, Self::Error> {
        match c {
            INLINE_CODE_DELIMITER => Ok(Self::Backtick),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq)]
enum BlockLocation {
    Outside,
    Inside,
    ClosingDelimiterFound,
}

trait BlockDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter;
}

#[derive(Debug)]
struct TripleBacktickDelimiter;
impl BlockDelimiter for TripleBacktickDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter { CodeBlockDelimiter::TripleBacktick }
}

#[derive(Debug)]
pub struct SingleBacktickDelimiter;
impl BlockDelimiter for SingleBacktickDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter { CodeBlockDelimiter::Backtick }
}

#[derive(Debug)]
struct BlockTracker<D: BlockDelimiter> {
    location:  BlockLocation,
    delimiter: D,
}

impl<D: BlockDelimiter> BlockTracker<D> {
    const fn new_with_delimiter(delimiter: D) -> Self {
        Self {
            location: BlockLocation::Outside,
            delimiter,
        }
    }

    /// One might notice that if we're at `BlockLocation::OnClosingDelimiter` and we
    /// encounter a delimiter, we go back to inside - this is intentional for the case
    /// where another code block is opened up right after the last one - it's possible in markdown
    /// so we don't treat this as a "nested" case we treat it as an opening of a code block
    fn update<T>(&mut self, content: T)
    where
        T: TryInto<CodeBlockDelimiter>,
    {
        if let Ok(delimiter) = content.try_into()
            && delimiter == self.delimiter.delimiter_type()
        {
            match self.location {
                BlockLocation::Inside => {
                    self.location = BlockLocation::ClosingDelimiterFound;
                },
                BlockLocation::Outside | BlockLocation::ClosingDelimiterFound => {
                    self.location = BlockLocation::Inside;
                },
            }
        } else if self.location == BlockLocation::ClosingDelimiterFound {
            self.location = BlockLocation::Outside;
        }
    }

    // we want to be clear that the `ClosingDelimiterFound` should also be skipped
    // if we didn't skip it then the closing `TripleBacktickDelimiter` ``` would be
    // considered "outside" and it would then be prased by the
    // character iterator and would treat this as an open/close/open of a code block
    const fn is_in_code_block(&self) -> bool {
        matches!(
            self.location,
            BlockLocation::Inside | BlockLocation::ClosingDelimiterFound
        )
    }

    fn is_inside(&self) -> bool { self.location == BlockLocation::Inside }
}

#[derive(Debug)]
pub(super) struct CodeBlockExcluder(BlockTracker<TripleBacktickDelimiter>);

impl CodeBlockExcluder {
    pub(super) const fn new() -> Self {
        Self(BlockTracker::new_with_delimiter(TripleBacktickDelimiter))
    }

    pub(super) fn update(&mut self, content: &str) { self.0.update(content); }

    pub(super) const fn is_in_code_block(&self) -> bool { self.0.is_in_code_block() }
}

#[derive(Debug)]
pub struct InlineCodeExcluder(BlockTracker<SingleBacktickDelimiter>);

impl InlineCodeExcluder {
    pub const fn new() -> Self { Self(BlockTracker::new_with_delimiter(SingleBacktickDelimiter)) }

    pub fn update(&mut self, content: char) { self.0.update(content); }

    pub const fn is_in_code_block(&self) -> bool { self.0.is_in_code_block() }

    pub fn is_inside(&self) -> bool { self.0.is_inside() }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::*;
    use crate::test_support;
    use crate::validated_config::ChangeMode;
    use crate::wikilink::InvalidWikilink;
    use crate::wikilink::InvalidWikilinkReason;

    #[test]
    fn test_code_block_tracking() {
        let mut tracker = CodeBlockExcluder::new();

        // Initial state
        assert!(!tracker.is_in_code_block(), "Initial state should not skip");

        tracker.update("```rust");
        assert!(tracker.is_in_code_block(), "Should skip inside code block");
        tracker.update("let x = 42;");
        assert!(tracker.is_in_code_block(), "Should still be in code block");
        tracker.update("```");
        assert!(
            tracker.is_in_code_block(),
            "Should skip while processing closing delimiter"
        );

        tracker.update("next line"); // This moves us to Outside
        assert!(
            !tracker.is_in_code_block(),
            "Should not skip after code block"
        );

        // Regular content
        tracker.update("Regular text");
        assert!(!tracker.is_in_code_block(), "Should not be in code block");

        // Nested code blocks (treated as toggles)
        tracker.update("```python");
        assert!(
            tracker.is_in_code_block(),
            "Should skip in second code block"
        );
        tracker.update("print('hello')");
        tracker.update("```");
        assert!(tracker.is_in_code_block(), "Should skip after second block");

        // immediately following with another code block opening
        tracker.update("```");
        assert!(
            tracker.is_in_code_block(),
            "Should skip after opening another code block right after the last one"
        );
    }

    #[test]
    fn test_inline_code_tracking() {
        let mut tracker = InlineCodeExcluder::new();

        // Initial state
        assert!(!tracker.is_in_code_block(), "Initial state should not skip");

        tracker.update('`');
        assert!(
            tracker.is_in_code_block(),
            "Should skip opening inline code block"
        );

        tracker.update('a');
        assert!(tracker.is_in_code_block(), "should skip inside code block");

        tracker.update('`');
        assert!(
            tracker.is_in_code_block(),
            "Should skip closing inline code block"
        );

        tracker.update('b');
        assert!(
            !tracker.is_in_code_block(),
            "Should not skip regular text after an inline code block"
        );
    }

    #[test]
    fn test_collect_exclusion_zones_with_invalid_wikilinks() {
        let (_, validated_config, mut obsidian_repository) = test_support::create_test_environment(
            ChangeMode::DryRun,
            None,
            None,
            Some("Text [[invalid|link|extra]] and more text"),
        );

        let markdown_file = obsidian_repository.markdown_files.first_mut().unwrap();

        // Add an invalid wikilink
        markdown_file.wikilinks.invalid.push(InvalidWikilink {
            content:     "[[invalid|link|extra]]".to_string(),
            reason:      InvalidWikilinkReason::DoubleAlias,
            span:        (5, 27),
            line:        "Text [[invalid|link|extra]] and more text".to_string(),
            line_number: 1,
        });

        let zones = markdown_file.collect_exclusion_zones(
            "Text [[invalid|link|extra]] and more text",
            &validated_config,
        );

        assert!(!zones.is_empty(), "Should have at least one exclusion zone");
        assert!(
            zones.contains(&(5, 27)),
            "Should contain invalid wikilink span"
        );
    }

    #[test]
    fn test_exclusion_zones_with_multiple_invalid_wikilinks() {
        let (_, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);

        let markdown_file = obsidian_repository.markdown_files.first_mut().unwrap();

        // Add multiple invalid wikilinks
        markdown_file.wikilinks.invalid.extend(vec![
            InvalidWikilink {
                content:     "[[test|one|two]]".to_string(),
                reason:      InvalidWikilinkReason::DoubleAlias,
                span:        (0, 16),
                line:        "[[test|one|two]] some text [[]]".to_string(),
                line_number: 1,
            },
            InvalidWikilink {
                content:     "[[]]".to_string(),
                reason:      InvalidWikilinkReason::Empty,
                span:        (27, 31),
                line:        "[[test|one|two]] some text [[]]".to_string(),
                line_number: 1,
            },
        ]);

        let zones = markdown_file
            .collect_exclusion_zones("[[test|one|two]] some text [[]]", &validated_config);

        assert_eq!(zones.len(), 2, "Should have two exclusion zones");
        assert!(
            zones.contains(&(0, 16)),
            "Should contain first invalid wikilink span"
        );
        assert!(
            zones.contains(&(27, 31)),
            "Should contain second invalid wikilink span"
        );
    }

    #[test]
    fn test_exclusion_zones_only_matches_current_line() {
        let (_, validated_config, mut obsidian_repository) = test_support::create_test_environment(
            ChangeMode::DryRun,
            None,
            None,
            Some("Line 1 with [[bad|link|here]]\nLine 2 with normal text"),
        );

        let markdown_file = obsidian_repository.markdown_files.first_mut().unwrap();

        // Add invalid wikilink from a different line
        markdown_file.wikilinks.invalid.push(InvalidWikilink {
            content:     "[[bad|link|here]]".to_string(),
            reason:      InvalidWikilinkReason::DoubleAlias,
            span:        (10, 26),
            line:        "Line 1 with [[bad|link|here]]".to_string(),
            line_number: 1,
        });

        // Check exclusion zones for line2
        let zones =
            markdown_file.collect_exclusion_zones("Line 2 with normal text", &validated_config);

        assert!(
            zones.is_empty(),
            "Should not have exclusion zones for different line"
        );
    }
}

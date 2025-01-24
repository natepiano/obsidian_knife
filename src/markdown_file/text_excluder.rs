#[derive(Debug, PartialEq)]
pub enum CodeBlockDelimiter {
    Backtick,
    TripleBacktick,
}

impl TryFrom<&str> for CodeBlockDelimiter {
    type Error = (); // Using unit type for error since we don't care if it fails

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.trim().starts_with("```") {
            Ok(CodeBlockDelimiter::TripleBacktick)
        } else {
            Err(())
        }
    }
}

impl TryFrom<char> for CodeBlockDelimiter {
    type Error = ();

    fn try_from(c: char) -> Result<Self, Self::Error> {
        match c {
            '`' => Ok(CodeBlockDelimiter::Backtick),
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

pub trait BlockDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter;
}

pub struct TripleBacktickDelimiter;
impl BlockDelimiter for TripleBacktickDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter {
        CodeBlockDelimiter::TripleBacktick
    }
}

pub struct SingleBacktickDelimiter;
impl BlockDelimiter for SingleBacktickDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter {
        CodeBlockDelimiter::Backtick
    }
}

#[derive(Debug)]
pub struct BlockTracker<D: BlockDelimiter> {
    location: BlockLocation,
    delimiter: D,
}

impl<D: BlockDelimiter> BlockTracker<D> {
    pub fn new_with_delimiter(delimiter: D) -> Self {
        Self {
            location: BlockLocation::Outside,
            delimiter,
        }
    }

    /// One might notice that if we're at BlockLocation::OnClosingDelimiter and we
    /// encounter a delimiter, we go back to inside - this is intentional for the case
    /// where another code block is opened up right after the last one - it's possible in markdown
    /// so we don't treat this as a "nested" case we treat it as an opening of a code block
    pub fn update<T>(&mut self, content: T)
    where
        T: TryInto<CodeBlockDelimiter>,
    {
        if let Ok(delimiter) = content.try_into() {
            if delimiter == self.delimiter.delimiter_type() {
                match self.location {
                    BlockLocation::Inside => {
                        self.location = BlockLocation::ClosingDelimiterFound;
                    }
                    BlockLocation::Outside => {
                        self.location = BlockLocation::Inside;
                    }
                    BlockLocation::ClosingDelimiterFound => {
                        self.location = BlockLocation::Inside;
                    }
                }
            }
        } else if self.location == BlockLocation::ClosingDelimiterFound {
            self.location = BlockLocation::Outside;
        }
    }

    // we want to be clear that the ClosingDelimiterFound should also be skipped
    // if we didn't skip it then the closing TripleBacktickDelimiter ``` would be
    // considered "outside" and it would then be prased by the
    // character iterator and would treat this as an open/close/open of a code block
    pub fn is_in_code_block(&self) -> bool {
        matches!(
            self.location,
            BlockLocation::Inside | BlockLocation::ClosingDelimiterFound
        )
    }

    pub fn is_inside(&self) -> bool {
        self.location == BlockLocation::Inside
    }
}

pub type CodeBlockExcluder = BlockTracker<TripleBacktickDelimiter>;
pub type InlineCodeExcluder = BlockTracker<SingleBacktickDelimiter>;

impl CodeBlockExcluder {
    pub fn new() -> Self {
        Self::new_with_delimiter(TripleBacktickDelimiter)
    }
}

impl InlineCodeExcluder {
    pub fn new() -> Self {
        Self::new_with_delimiter(SingleBacktickDelimiter)
    }
}

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

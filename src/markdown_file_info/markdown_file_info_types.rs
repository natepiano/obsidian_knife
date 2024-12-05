use crate::frontmatter::FrontMatter;
use crate::markdown_file_info::extract_date;
use crate::wikilink::is_wikilink;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PersistReason {
    DateCreatedUpdated { reason: DateValidationIssue },
    DateModifiedUpdated { reason: DateValidationIssue },
    DateCreatedFixApplied,
    BackPopulated,
    ImageReferencesModified,
}

impl fmt::Display for PersistReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistReason::DateCreatedUpdated { .. } => write!(f, "date_created updated"),
            PersistReason::DateModifiedUpdated { .. } => write!(f, "date_modified updated"),
            PersistReason::DateCreatedFixApplied => write!(f, "date_created_fix applied"),
            PersistReason::BackPopulated => write!(f, "back populated"),
            PersistReason::ImageReferencesModified => write!(f, "image references updated"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateValidationIssue {
    Missing,
    InvalidDateFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

impl fmt::Display for DateValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            DateValidationIssue::Missing => "missing",
            DateValidationIssue::InvalidDateFormat => "invalid date format",
            DateValidationIssue::InvalidWikilink => "invalid wikilink",
            DateValidationIssue::FileSystemMismatch => "doesn't match file system",
        };
        write!(f, "{}", description)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DateValidation {
    pub frontmatter_date: Option<String>,
    pub file_system_date: DateTime<Utc>,
    pub issue: Option<DateValidationIssue>,
    pub operational_timezone: String,
}
// In markdown_file_info.rs
#[derive(Debug, Clone)]
pub struct DateCreatedFixValidation {
    pub date_string: Option<String>,
    pub fix_date: Option<DateTime<Utc>>,
}

impl DateCreatedFixValidation {
    pub(crate) fn from_frontmatter(
        frontmatter: &Option<FrontMatter>,
        file_created_date: DateTime<Utc>,
    ) -> Self {
        let date_string = frontmatter
            .as_ref()
            .and_then(|fm| fm.date_created_fix().cloned());

        let parsed_date = date_string.as_ref().and_then(|date_str| {
            let date = if is_wikilink(Some(date_str)) {
                extract_date(date_str)
            } else {
                date_str.trim().trim_matches('"')
            };

            NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
                .ok()
                .map(|naive_date| {
                    let time = file_created_date.time();
                    let naive_datetime = naive_date.and_time(time);
                    Utc.from_local_datetime(&naive_datetime).unwrap()
                })
        });

        DateCreatedFixValidation {
            date_string,
            fix_date: parsed_date,
        }
    }
}
pub trait ReplaceableMatch {
    fn line_number(&self) -> usize;
    fn position(&self) -> usize;
    fn get_replacement(&self) -> String;
    fn matched_text(&self) -> String;
}

#[derive(Clone, Debug, Default)]
pub struct BackPopulateMatch {
    pub found_text: String,
    pub frontmatter_line_count: usize,
    pub in_markdown_table: bool,
    pub line_number: usize,
    pub line_text: String,
    pub position: usize,
    pub relative_path: String,
    pub replacement: String,
}

impl ReplaceableMatch for BackPopulateMatch {
    fn line_number(&self) -> usize {
        self.line_number
    }

    fn position(&self) -> usize {
        self.position
    }

    fn get_replacement(&self) -> String {
        self.replacement.clone()
    }

    fn matched_text(&self) -> String {
        self.found_text.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct BackPopulateMatches {
    pub ambiguous: Vec<BackPopulateMatch>,
    pub unambiguous: Vec<BackPopulateMatch>,
}

#[derive(Debug)]
pub struct FileProcessingState {
    in_frontmatter: bool,
    in_code_block: bool,
    frontmatter_delimiter_count: usize,
}

impl FileProcessingState {
    pub(crate) fn new() -> Self {
        Self {
            in_frontmatter: false,
            in_code_block: false,
            frontmatter_delimiter_count: 0,
        }
    }

    pub(crate) fn update_for_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();

        // Check frontmatter delimiter
        if trimmed == "---" {
            self.frontmatter_delimiter_count += 1;
            self.in_frontmatter = self.frontmatter_delimiter_count % 2 != 0;
            return true;
        }

        // Check code block delimiter if not in frontmatter
        if !self.in_frontmatter && trimmed.starts_with("```") {
            self.in_code_block = !self.in_code_block;
            return true;
        }

        // Return true if we should skip this line
        self.in_frontmatter || self.in_code_block
    }

    pub(crate) fn should_skip_line(&self) -> bool {
        self.in_frontmatter || self.in_code_block
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageLinkTarget {
    Internal,
    External,
}
#[derive(Debug, Clone, PartialEq)]
pub enum ImageLinkRendering {
    LinkOnly,
    Embedded,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageLinkType {
    Wikilink(ImageLinkRendering),
    MarkdownLink(ImageLinkTarget, ImageLinkRendering),
    // RawHTTP,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImageLinks {
    pub found: Vec<ImageLink>,    // All valid image links
    pub missing: Vec<ImageLink>,  // References to non-existent images
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageLink {
    pub image_link_type: ImageLinkType,
    pub line_number: usize,
    pub position: usize,
    pub raw_link: String, // The full ![[image.jpg]] syntax
    pub filename: String, // Just "image.jpg"
}

// todo - if we store the line and position info of the link, couldn't we automatically mark those
//        exclusion zones - then we'd have to store all of them - right now we don't store all because
//        we don't want them to be considered for reference processing (for example, external links)
//        but instead we could store all and just filter out the ones we want to consider for image reference checking
//        probably just a bool on ImageLink that says "check_image_reference"

// handle links of type ![[somefile.png]] or ![[somefile.png|300]] or ![alt](somefile.png)
impl ImageLink {
    pub fn new(raw_link: String, line_number: usize, position: usize) -> Self {
        // // Handle Raw HTTP style: starts with http:// or https://
        // if raw_link.starts_with("http://") || raw_link.starts_with("https://") {
        //     let filename = raw_link.rsplit('/').next().unwrap_or("").to_lowercase();
        //     return Self {
        //         image_link_type: ImageLinkType::RawHTTP,
        //         raw_link,
        //         filename,
        //     };
        // }

        // Handle Wikilink style: [[image.png]] or ![[image.png]]
        if raw_link.ends_with("]]") {
            let rendering = if raw_link.starts_with("!") {
                ImageLinkRendering::Embedded
            } else {
                ImageLinkRendering::LinkOnly
            };

            let filename = raw_link
                .trim_start_matches('!')
                .trim_start_matches("[[")
                .trim_end_matches("]]")
                .split('|')
                .next()
                .unwrap_or("")
                .trim() // Add trim here to remove whitespace
                .trim_matches('\\') // Add this to remove any escape characters
                .to_lowercase();

            return Self {
                image_link_type: ImageLinkType::Wikilink(rendering),
                line_number,
                position,
                raw_link,
                filename,
            };
        }

        // Handle Markdown style: ![alt](image.png) or [alt](image.png)
        if raw_link.ends_with(")") {
            let rendering = if raw_link.starts_with("!") {
                ImageLinkRendering::Embedded
            } else {
                ImageLinkRendering::LinkOnly
            };

            // Extract the URL part between () brackets
            let start = raw_link.find("](").map(|i| i + 2).unwrap_or(0);
            let url = &raw_link[start..raw_link.len() - 1];

            let location = if url.starts_with("http://") || url.starts_with("https://") {
                ImageLinkTarget::External
            } else {
                ImageLinkTarget::Internal
            };

            let filename = if location == ImageLinkTarget::Internal {
                url.rsplit('/').next().unwrap_or("").to_lowercase()
            } else {
                url.to_lowercase()
            };

            return Self {
                image_link_type: ImageLinkType::MarkdownLink(location, rendering),
                line_number,
                position,
                raw_link,
                filename,
            };
        }

        // Invalid/unrecognized format
        panic!(
            "Invalid image link format passed to ImageLink::new(): {}",
            raw_link
        );
    }
}

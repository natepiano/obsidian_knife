use crate::frontmatter::FrontMatter;
use crate::image_file::IncompatibilityReason;
use crate::{markdown_file, obsidian_repository,wikilink};
use crate::wikilink::{InvalidWikilink, Wikilink};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use std::fmt;
use std::ops::Deref;
use std::path::PathBuf;

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
// In markdown_file.rs
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
            let date = if wikilink::is_wikilink(Some(date_str)) {
                markdown_file::extract_date(date_str)
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

#[derive(Debug, Clone, PartialEq)]
pub enum MatchType {
    BackPopulate,
    ImageReference,
}

pub trait ReplaceableContent {
    fn line_number(&self) -> usize;
    fn position(&self) -> usize;
    fn get_replacement(&self) -> String;
    fn matched_text(&self) -> String;
    fn match_type(&self) -> MatchType;
}

#[derive(Clone, Debug, Default)]
pub struct BackPopulateMatch {
    pub found_text: String,
    pub in_markdown_table: bool,
    pub line_number: usize,
    pub line_text: String,
    pub position: usize,
    pub relative_path: String,
    pub replacement: String,
}

impl ReplaceableContent for BackPopulateMatch {
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

    fn match_type(&self) -> MatchType {
        MatchType::BackPopulate
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
pub struct Wikilinks {
    pub valid: Vec<Wikilink>,
    pub invalid: Vec<InvalidWikilink>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImageLinks {
    pub links: Vec<ImageLink>,

}

impl ImageLinks {
    pub fn iter(&self) -> impl Iterator<Item = &ImageLink> {
        self.links.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ImageLink> {
        self.links.iter_mut()
    }
}

impl Deref for ImageLinks {
    type Target = Vec<ImageLink>;

    fn deref(&self) -> &Self::Target {
        &self.links
    }
}

impl<'a> IntoIterator for &'a ImageLinks {
    type Item = &'a ImageLink;
    type IntoIter = std::slice::Iter<'a, ImageLink>;

    fn into_iter(self) -> Self::IntoIter {
        self.links.iter()
    }
}

impl<'a> IntoIterator for &'a mut ImageLinks {
    type Item = &'a mut ImageLink;
    type IntoIter = std::slice::IterMut<'a, ImageLink>;

    fn into_iter(self) -> Self::IntoIter {
        self.links.iter_mut()
    }
}

impl FromIterator<ImageLink> for ImageLinks {
    fn from_iter<I: IntoIterator<Item = ImageLink>>(iter: I) -> Self {
        ImageLinks {
            links: iter.into_iter().collect(),
        }
    }
}

impl ImageLinks {
    pub fn missing(&self) -> ImageLinks {
        self.links
            .iter()
            .filter(|image_link| image_link.state == ImageLinkState::Missing)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum ImageLinkState {
    #[default]
    Found,   // Image exists and is valid
    Missing, // Image doesn't exist
    Duplicate {
        keeper_path: PathBuf, // Path to the image we should reference instead
    },
    Incompatible {
        reason: IncompatibilityReason, // Why the referenced image should be removed
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageLink {
    pub matched_text: String, // The full ![[image.jpg]] syntax
    pub position: usize,
    pub line_number: usize,
    pub filename: String, // Just "image.jpg"
    pub relative_path: String,
    pub alt_text: String,
    pub size_parameter: Option<String>, // Added to handle |400 style parameters
    pub state: ImageLinkState,
    pub image_link_type: ImageLinkType,
}

impl ReplaceableContent for ImageLink {
    fn line_number(&self) -> usize {
        self.line_number
    }

    fn position(&self) -> usize {
        self.position
    }

    fn get_replacement(&self) -> String {
        match &self.state {
            ImageLinkState::Found => self.matched_text.clone(),
            ImageLinkState::Missing => String::new(),
            ImageLinkState::Incompatible { .. } => String::new(),
            ImageLinkState::Duplicate { keeper_path } => {
                let new_name = keeper_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                let new_relative = format!("{}/{}", self.relative_path, new_name);

                match &self.image_link_type {
                    ImageLinkType::Wikilink(rendering) => match rendering {
                        ImageLinkRendering::Embedded => match &self.size_parameter {
                            Some(size) => format!("![[{}|{}]]", new_relative, size),
                            None => format!("![[{}]]", new_relative),
                        },
                        ImageLinkRendering::LinkOnly => format!("[[{}]]", new_relative),
                    },
                    ImageLinkType::MarkdownLink(target, rendering) => {
                        match (target, rendering) {
                            (ImageLinkTarget::Internal, ImageLinkRendering::Embedded) => {
                                format!("![{}]({})", self.alt_text, new_relative)
                            }
                            (ImageLinkTarget::Internal, ImageLinkRendering::LinkOnly) => {
                                format!("[{}]({})", self.alt_text, new_relative)
                            }
                            (ImageLinkTarget::External, _) => {
                                // We shouldn't get here for duplicate handling as we don't process external images
                                self.matched_text.clone()
                            }
                        }
                    }
                }
            }
        }
    }

    fn matched_text(&self) -> String {
        self.matched_text.clone()
    }

    fn match_type(&self) -> MatchType {
        MatchType::ImageReference
    }
}

// handle links of type ![[somefile.png]] or ![[somefile.png|300]] or ![alt](somefile.png)
impl ImageLink {
    pub fn new(raw_link: String, line_number: usize, position: usize) -> Self {
        let relative_path = obsidian_repository::extract_relative_path(&raw_link);

        // Determine link type and rendering first
        let (filename, image_link_type, alt_text, size_parameter) = if raw_link.ends_with("]]") {
            // Wikilink style
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
                .trim()
                .trim_matches('\\')
                .to_lowercase();

            let size_parameter = raw_link
                .split('|')
                .nth(1)
                .map(|s| s.trim_end_matches("]]").to_string());

            (
                filename,
                ImageLinkType::Wikilink(rendering),
                String::new(),
                size_parameter,
            )
        } else if raw_link.ends_with(")") {
            // Markdown style
            let rendering = if raw_link.starts_with("!") {
                ImageLinkRendering::Embedded
            } else {
                ImageLinkRendering::LinkOnly
            };

            let alt_text = raw_link
                .find("](")
                .map(|alt_end| raw_link[2..alt_end].to_string())
                .unwrap_or_default();

            let url_start = raw_link.find("](").map(|i| i + 2).unwrap_or(0);
            let url = &raw_link[url_start..raw_link.len() - 1];

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

            (
                filename,
                ImageLinkType::MarkdownLink(location, rendering),
                alt_text,
                None,
            )
        } else {
            panic!(
                "Invalid image link format passed to ImageLink::new(): {}",
                raw_link
            );
        };

        Self {
            matched_text: raw_link,
            position,
            line_number,
            filename,
            relative_path,
            alt_text,
            size_parameter,
            state: ImageLinkState::default(),
            image_link_type,
        }
    }
}

use std::fmt;
use std::path::PathBuf;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;

use super::date_validation;
use crate::constants::CLOSING_WIKILINK;
use crate::constants::DEFAULT_MEDIA_PATH;
use crate::constants::FORWARD_SLASH;
use crate::constants::OPENING_BRACKET;
use crate::constants::OPENING_PAREN;
use crate::constants::OPENING_WIKILINK;
use crate::frontmatter::FrontMatter;
use crate::image_file::IncompatibilityReason;
use crate::utils::EnumFilter;
use crate::wikilink;
use crate::wikilink::InvalidWikilink;
use crate::wikilink::Wikilink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistReason {
    DateCreatedUpdated { reason: DateValidationIssue },
    DateModifiedUpdated { reason: DateValidationIssue },
    DateCreatedFixApplied,
    BackPopulated,
    FrontmatterCreated,
    ImageReferencesModified,
}

impl fmt::Display for PersistReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DateCreatedUpdated { .. } => write!(f, "date_created updated"),
            Self::DateModifiedUpdated { .. } => write!(f, "date_modified updated"),
            Self::DateCreatedFixApplied => write!(f, "date_created_fix applied"),
            Self::BackPopulated => write!(f, "back populated"),
            Self::FrontmatterCreated => write!(f, "frontmatter created"),
            Self::ImageReferencesModified => write!(f, "image references updated"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateValidationIssue {
    Missing,
    InvalidDateFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

impl fmt::Display for DateValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            Self::Missing => "missing",
            Self::InvalidDateFormat => "invalid date format",
            Self::InvalidWikilink => "invalid wikilink",
            Self::FileSystemMismatch => "doesn't match file system",
        };
        write!(f, "{description}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateValidation {
    pub frontmatter_date:     Option<String>,
    pub file_system_date:     DateTime<Utc>,
    pub issue:                Option<DateValidationIssue>,
    pub operational_timezone: String,
}

impl DateValidation {
    pub fn operational_file_system_date(&self) -> DateTime<Utc> {
        self.operational_timezone
            .parse::<chrono_tz::Tz>()
            .map_or(self.file_system_date, |tz| {
                let local = self.file_system_date.with_timezone(&tz);
                let naive = local.naive_local();
                DateTime::from_naive_utc_and_offset(naive, Utc)
            })
    }
}
// In markdown_file.rs
#[derive(Debug, Default, Clone)]
pub struct DateCreatedFixValidation {
    #[cfg(test)]
    pub date_string: Option<String>,
    pub fix_date:    Option<DateTime<Utc>>,
}

impl DateCreatedFixValidation {
    pub(super) fn from_frontmatter(
        frontmatter: Option<&FrontMatter>,
        file_created_date: DateTime<Utc>,
        operational_timezone: &str,
    ) -> Self {
        let fix_str = frontmatter.and_then(|fm| fm.date_created_fix().map(String::from));

        let parsed_date = fix_str.as_ref().and_then(|date_str| {
            let date = if wikilink::is_wikilink(Some(date_str)) {
                date_validation::extract_date(date_str)
            } else {
                date_str.trim().trim_matches('"')
            };

            // First parse the date string
            NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
                .ok()
                .map(|naive_date| {
                    let tz: chrono_tz::Tz = operational_timezone.parse().unwrap_or(chrono_tz::UTC);

                    // Create naive datetime at noon to ensure date consistency
                    #[allow(clippy::unwrap_used, reason = "noon (12:00:00) is always a valid time")]
                    let naive_datetime = naive_date.and_hms_opt(12, 0, 0).unwrap();

                    // Convert to UTC
                    let fixed_date = tz
                        .from_local_datetime(&naive_datetime)
                        .single()
                        .map_or_else(|| file_created_date, |dt| dt.with_timezone(&Utc));

                    // Assert that the date in operational timezone matches the requested fix date
                    let fixed_date_local = fixed_date.with_timezone(&tz);
                    assert_eq!(
                        fixed_date_local.date_naive(),
                        naive_date,
                        "Date mismatch: fixed_date converts to {} in {} but should be {}",
                        fixed_date_local.date_naive(),
                        operational_timezone,
                        naive_date
                    );

                    fixed_date
                })
        });

        Self {
            #[cfg(test)]
            date_string:              fix_str,
            fix_date:                 parsed_date,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MatchContext {
    #[default]
    Plaintext,
    MarkdownTable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub found_text:    String,
    pub match_context: MatchContext,
    pub line_number:   usize,
    pub line_text:     String,
    pub position:      usize,
    pub relative_path: String,
    pub replacement:   String,
}

impl ReplaceableContent for BackPopulateMatch {
    fn line_number(&self) -> usize { self.line_number }

    fn position(&self) -> usize { self.position }

    fn get_replacement(&self) -> String { self.replacement.clone() }

    fn matched_text(&self) -> String { self.found_text.clone() }

    fn match_type(&self) -> MatchType { MatchType::BackPopulate }
}

#[derive(Clone, Debug, Default)]
pub struct BackPopulateMatches {
    pub ambiguous:   Vec<BackPopulateMatch>,
    pub unambiguous: Vec<BackPopulateMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLinkTarget {
    Internal,
    External,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLinkRendering {
    LinkOnly,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLinkType {
    Wikilink(ImageLinkRendering),
    MarkdownLink(ImageLinkTarget, ImageLinkRendering),
    // RawHTTP,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Wikilinks {
    pub valid:   Vec<Wikilink>,
    pub invalid: Vec<InvalidWikilink>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deref, DerefMut, IntoIterator)]
pub struct ImageLinks {
    #[deref]
    #[deref_mut]
    #[into_iterator]
    pub links: Vec<ImageLink>,
}

impl FromIterator<ImageLink> for ImageLinks {
    fn from_iter<I: IntoIterator<Item = ImageLink>>(iter: I) -> Self {
        Self {
            links: iter.into_iter().collect(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ImageLinkState {
    #[default]
    Found, // Image exists and is valid
    Missing, // Image doesn't exist
    Duplicate {
        keeper_path: PathBuf, // Path to the image we should reference instead
    },
    Incompatible {
        reason: IncompatibilityReason, // Why the referenced image should be removed
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLink {
    pub matched_text:   String, // The full ![[image.jpg]] syntax
    pub position:       usize,
    pub line_number:    usize,
    pub filename:       String, // Just "image.jpg"
    pub relative_path:  String,
    pub alt_text:       String,
    pub size_parameter: Option<String>, // Added to handle |400 style parameters
    pub state:          ImageLinkState,
    pub link_type:      ImageLinkType,
}

impl EnumFilter for ImageLink {
    type EnumType = ImageLinkState;

    fn as_enum(&self) -> &Self::EnumType { &self.state }
}

impl ReplaceableContent for ImageLink {
    fn line_number(&self) -> usize { self.line_number }

    fn position(&self) -> usize { self.position }

    fn get_replacement(&self) -> String {
        match &self.state {
            ImageLinkState::Found => self.matched_text.clone(),
            ImageLinkState::Missing | ImageLinkState::Incompatible { .. } => String::new(),
            ImageLinkState::Duplicate { keeper_path } => {
                let new_name = keeper_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                let new_relative = format!("{}/{new_name}", self.relative_path);

                match &self.link_type {
                    ImageLinkType::Wikilink(rendering) => match rendering {
                        ImageLinkRendering::Embedded => self.size_parameter.as_ref().map_or_else(
                            || format!("![[{new_relative}]]"),
                            |size| format!("![[{new_relative}|{size}]]"),
                        ),
                        ImageLinkRendering::LinkOnly => format!("[[{new_relative}]]"),
                    },
                    ImageLinkType::MarkdownLink(target, rendering) => {
                        match (target, rendering) {
                            (ImageLinkTarget::Internal, ImageLinkRendering::Embedded) => {
                                format!("![{}]({new_relative})", self.alt_text)
                            },
                            (ImageLinkTarget::Internal, ImageLinkRendering::LinkOnly) => {
                                format!("[{}]({new_relative})", self.alt_text)
                            },
                            (ImageLinkTarget::External, _) => {
                                // We shouldn't get here for duplicate handling as we don't process
                                // external images
                                self.matched_text.clone()
                            },
                        }
                    },
                }
            },
        }
    }

    fn matched_text(&self) -> String { self.matched_text.clone() }

    fn match_type(&self) -> MatchType { MatchType::ImageReference }
}

// handle links of type ![[somefile.png]] or ![[somefile.png|300]] or ![alt](somefile.png)
impl ImageLink {
    #[allow(
        clippy::panic,
        reason = "invalid image link format indicates a bug in the regex that calls this constructor"
    )]
    pub fn new(raw_link: String, line_number: usize, position: usize) -> Self {
        let relative_path = extract_relative_path(&raw_link);

        // Determine link type and rendering first
        let (filename, image_link_type, alt_text, size_parameter) =
            if raw_link.ends_with(CLOSING_WIKILINK) {
                // Wikilink style
                let rendering = if raw_link.starts_with('!') {
                    ImageLinkRendering::Embedded
                } else {
                    ImageLinkRendering::LinkOnly
                };

                let filename = raw_link
                    .trim_start_matches('!')
                    .trim_start_matches(OPENING_WIKILINK)
                    .trim_end_matches(CLOSING_WIKILINK)
                    .split('|')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('\\')
                    .to_lowercase();

                let size_parameter = raw_link
                    .split('|')
                    .nth(1)
                    .map(|s| s.trim_end_matches(CLOSING_WIKILINK).to_string());

                (
                    filename,
                    ImageLinkType::Wikilink(rendering),
                    String::new(),
                    size_parameter,
                )
            } else if raw_link.ends_with(')') {
                // Markdown style
                let rendering = if raw_link.starts_with('!') {
                    ImageLinkRendering::Embedded
                } else {
                    ImageLinkRendering::LinkOnly
                };

                let alt_text = raw_link
                    .find("](")
                    .map(|alt_end| raw_link[2..alt_end].to_string())
                    .unwrap_or_default();

                let url_start = raw_link.find("](").map_or(0, |i| i + 2);
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
                panic!("Invalid image link format passed to ImageLink::new(): {raw_link}");
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
            link_type: image_link_type,
        }
    }
}

// for deletion, we need the path to the file
fn extract_relative_path(matched: &str) -> String {
    if !matched.contains(FORWARD_SLASH) {
        return DEFAULT_MEDIA_PATH.to_string();
    }

    // Extract the portion before the last '/' (potential path).
    let prefix = matched
        .rsplit_once(FORWARD_SLASH)
        .map_or(matched, |(prefix, _)| prefix);

    // Find the position of the last opening '(' or '[' and take the path after it.
    prefix
        .rfind(|c| matches!(c, OPENING_PAREN | OPENING_BRACKET))
        .map(|pos| &prefix[pos + 1..])
        .map(|p| p.trim_end_matches(FORWARD_SLASH))
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_MEDIA_PATH)
        .to_string()
}

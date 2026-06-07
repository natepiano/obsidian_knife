use std::ffi::OsStr;
use std::path::PathBuf;

use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;

use super::constants::HTTP_URL_PREFIX;
use super::constants::HTTPS_URL_PREFIX;
use super::constants::IMAGE_LINK_SIZE_PARAMETER_INDEX;
use super::constants::INVALID_IMAGE_LINK_FORMAT_PREFIX;
use super::replaceable_content::MatchType;
use super::replaceable_content::ReplaceableContent;
use crate::constants::BACKSLASH;
use crate::constants::CLOSING_PAREN;
use crate::constants::CLOSING_WIKILINK;
use crate::constants::DEFAULT_MEDIA_PATH;
use crate::constants::FORWARD_SLASH;
use crate::constants::IMAGE_EMBED_MARKER;
use crate::constants::IMAGE_LINK_PREFIX;
use crate::constants::MARKDOWN_LINK_SEPARATOR;
use crate::constants::OPENING_BRACKET;
use crate::constants::OPENING_PAREN;
use crate::constants::OPENING_WIKILINK;
use crate::constants::PIPE;
use crate::image_file::IncompatibilityReason;
use crate::support::EnumFilter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLinkTarget {
    Internal,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageRendering {
    Linked,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLinkType {
    Wiki(ImageRendering),
    Markdown(ImageLinkTarget, ImageRendering),
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
    Found,
    Missing,
    Duplicate {
        keeper_path: PathBuf,
    },
    Incompatible {
        reason: IncompatibilityReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLink {
    pub matched_text:   String,
    pub position:       usize,
    pub line_number:    usize,
    pub filename:       String,
    pub relative_path:  String,
    pub alt_text:       String,
    pub size_parameter: Option<String>,
    pub state:          ImageLinkState,
    pub link_type:      ImageLinkType,
}

struct ParsedImageLink {
    filename:       String,
    link_type:      ImageLinkType,
    alt_text:       String,
    size_parameter: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RawImageLinkSyntax {
    Wiki,
    Markdown,
    Invalid,
}

impl From<&str> for RawImageLinkSyntax {
    fn from(raw_link: &str) -> Self {
        match (
            raw_link.ends_with(CLOSING_WIKILINK),
            raw_link.ends_with(CLOSING_PAREN),
        ) {
            (true, _) => Self::Wiki,
            (false, true) => Self::Markdown,
            (false, false) => Self::Invalid,
        }
    }
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
                    .and_then(OsStr::to_str)
                    .unwrap_or_default();
                let new_relative = format!("{}{FORWARD_SLASH}{new_name}", self.relative_path);

                match &self.link_type {
                    ImageLinkType::Wiki(rendering) => match rendering {
                        ImageRendering::Embedded => self.size_parameter.as_ref().map_or_else(
                            || {
                                format!(
                                    "{IMAGE_EMBED_MARKER}{OPENING_WIKILINK}{new_relative}{CLOSING_WIKILINK}"
                                )
                            },
                            |size| {
                                format!(
                                    "{IMAGE_EMBED_MARKER}{OPENING_WIKILINK}{new_relative}{PIPE}{size}{CLOSING_WIKILINK}"
                                )
                            },
                        ),
                        ImageRendering::Linked => {
                            format!("{OPENING_WIKILINK}{new_relative}{CLOSING_WIKILINK}")
                        },
                    },
                    ImageLinkType::Markdown(target, rendering) => match (target, rendering) {
                        (ImageLinkTarget::Internal, ImageRendering::Embedded) => {
                            format!(
                                "{IMAGE_LINK_PREFIX}{}{MARKDOWN_LINK_SEPARATOR}{new_relative}{CLOSING_PAREN}",
                                self.alt_text
                            )
                        },
                        (ImageLinkTarget::Internal, ImageRendering::Linked) => {
                            format!(
                                "{OPENING_BRACKET}{}{MARKDOWN_LINK_SEPARATOR}{new_relative}{CLOSING_PAREN}",
                                self.alt_text
                            )
                        },
                        (ImageLinkTarget::External, _) => self.matched_text.clone(),
                    },
                }
            },
        }
    }

    fn matched_text(&self) -> String { self.matched_text.clone() }

    fn match_type(&self) -> MatchType { MatchType::ImageReference }
}

impl ImageLink {
    pub fn new(raw_link: String, line_number: usize, position: usize) -> Result<Self, String> {
        let relative_path = extract_relative_path(&raw_link);

        let parsed_link = match RawImageLinkSyntax::from(raw_link.as_str()) {
            RawImageLinkSyntax::Wiki => parse_wiki_image_link(&raw_link),
            RawImageLinkSyntax::Markdown => parse_markdown_image_link(&raw_link),
            RawImageLinkSyntax::Invalid => {
                return Err(format!("{INVALID_IMAGE_LINK_FORMAT_PREFIX}{raw_link}"));
            },
        };

        Ok(Self {
            matched_text: raw_link,
            position,
            line_number,
            filename: parsed_link.filename,
            relative_path,
            alt_text: parsed_link.alt_text,
            size_parameter: parsed_link.size_parameter,
            state: ImageLinkState::default(),
            link_type: parsed_link.link_type,
        })
    }
}

fn image_rendering(raw_link: &str) -> ImageRendering {
    if raw_link.starts_with(IMAGE_EMBED_MARKER) {
        ImageRendering::Embedded
    } else {
        ImageRendering::Linked
    }
}

fn parse_wiki_image_link(raw_link: &str) -> ParsedImageLink {
    let rendering = image_rendering(raw_link);

    let filename = raw_link
        .trim_start_matches(IMAGE_EMBED_MARKER)
        .trim_start_matches(OPENING_WIKILINK)
        .trim_end_matches(CLOSING_WIKILINK)
        .split(PIPE)
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(BACKSLASH)
        .to_lowercase();

    let size_parameter = raw_link
        .split(PIPE)
        .nth(IMAGE_LINK_SIZE_PARAMETER_INDEX)
        .map(|s| s.trim_end_matches(CLOSING_WIKILINK).to_string());

    ParsedImageLink {
        filename,
        link_type: ImageLinkType::Wiki(rendering),
        alt_text: String::new(),
        size_parameter,
    }
}

fn parse_markdown_image_link(raw_link: &str) -> ParsedImageLink {
    let rendering = image_rendering(raw_link);

    let alt_text = raw_link
        .find(MARKDOWN_LINK_SEPARATOR)
        .map(|alt_end| raw_link[IMAGE_LINK_PREFIX.len()..alt_end].to_string())
        .unwrap_or_default();

    let url_start = raw_link
        .find(MARKDOWN_LINK_SEPARATOR)
        .map_or(0, |index| index + MARKDOWN_LINK_SEPARATOR.len());
    let url = &raw_link[url_start..raw_link.len() - 1];

    let target = if url.starts_with(HTTP_URL_PREFIX) || url.starts_with(HTTPS_URL_PREFIX) {
        ImageLinkTarget::External
    } else {
        ImageLinkTarget::Internal
    };

    let filename = match target {
        ImageLinkTarget::Internal => url
            .rsplit(FORWARD_SLASH)
            .next()
            .unwrap_or("")
            .to_lowercase(),
        ImageLinkTarget::External => url.to_lowercase(),
    };

    ParsedImageLink {
        filename,
        link_type: ImageLinkType::Markdown(target, rendering),
        alt_text,
        size_parameter: None,
    }
}

fn extract_relative_path(matched: &str) -> String {
    if !matched.contains(FORWARD_SLASH) {
        return DEFAULT_MEDIA_PATH.to_string();
    }

    let prefix = matched
        .rsplit_once(FORWARD_SLASH)
        .map_or(matched, |(prefix, _)| prefix);

    prefix
        .rfind(|character| matches!(character, OPENING_PAREN | OPENING_BRACKET))
        .map(|index| &prefix[index + 1..])
        .map(|path| path.trim_end_matches(FORWARD_SLASH))
        .filter(|path| !path.is_empty())
        .unwrap_or(DEFAULT_MEDIA_PATH)
        .to_string()
}

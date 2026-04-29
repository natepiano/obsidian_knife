use std::ffi::OsStr;
use std::path::PathBuf;

use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;

use super::replaceable_content::MatchType;
use super::replaceable_content::ReplaceableContent;
use crate::constants::CLOSING_WIKILINK;
use crate::constants::DEFAULT_MEDIA_PATH;
use crate::constants::FORWARD_SLASH;
use crate::constants::IMAGE_LINK_PREFIX;
use crate::constants::MARKDOWN_LINK_SEPARATOR;
use crate::constants::OPENING_BRACKET;
use crate::constants::OPENING_PAREN;
use crate::constants::OPENING_WIKILINK;
use crate::image_file::IncompatibilityReason;
use crate::utils::EnumFilter;
use crate::wikilink::InvalidWikilink;
use crate::wikilink::Wikilink;

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
    Wiki(ImageLinkRendering),
    Markdown(ImageLinkTarget, ImageLinkRendering),
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
                let new_relative = format!("{}/{new_name}", self.relative_path);

                match &self.link_type {
                    ImageLinkType::Wiki(rendering) => match rendering {
                        ImageLinkRendering::Embedded => self.size_parameter.as_ref().map_or_else(
                            || format!("![[{new_relative}]]"),
                            |size| format!("![[{new_relative}|{size}]]"),
                        ),
                        ImageLinkRendering::LinkOnly => format!("[[{new_relative}]]"),
                    },
                    ImageLinkType::Markdown(target, rendering) => match (target, rendering) {
                        (ImageLinkTarget::Internal, ImageLinkRendering::Embedded) => {
                            format!("![{}]({new_relative})", self.alt_text)
                        },
                        (ImageLinkTarget::Internal, ImageLinkRendering::LinkOnly) => {
                            format!("[{}]({new_relative})", self.alt_text)
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

        let (filename, link_type, alt_text, size_parameter) =
            if raw_link.ends_with(CLOSING_WIKILINK) {
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
                    ImageLinkType::Wiki(rendering),
                    String::new(),
                    size_parameter,
                )
            } else if raw_link.ends_with(')') {
                let rendering = if raw_link.starts_with('!') {
                    ImageLinkRendering::Embedded
                } else {
                    ImageLinkRendering::LinkOnly
                };

                let alt_text = raw_link
                    .find(MARKDOWN_LINK_SEPARATOR)
                    .map(|alt_end| raw_link[IMAGE_LINK_PREFIX.len()..alt_end].to_string())
                    .unwrap_or_default();

                let url_start = raw_link
                    .find(MARKDOWN_LINK_SEPARATOR)
                    .map_or(0, |index| index + MARKDOWN_LINK_SEPARATOR.len());
                let url = &raw_link[url_start..raw_link.len() - 1];

                let target = if url.starts_with("http://") || url.starts_with("https://") {
                    ImageLinkTarget::External
                } else {
                    ImageLinkTarget::Internal
                };

                let filename = if target == ImageLinkTarget::Internal {
                    url.rsplit('/').next().unwrap_or("").to_lowercase()
                } else {
                    url.to_lowercase()
                };

                (
                    filename,
                    ImageLinkType::Markdown(target, rendering),
                    alt_text,
                    None,
                )
            } else {
                return Err(format!(
                    "invalid image link format passed to ImageLink::new: {raw_link}"
                ));
            };

        Ok(Self {
            matched_text: raw_link,
            position,
            line_number,
            filename,
            relative_path,
            alt_text,
            size_parameter,
            state: ImageLinkState::default(),
            link_type,
        })
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

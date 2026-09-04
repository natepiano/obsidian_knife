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

/// How a note spelled the directory part of an image link. Obsidian resolves a bare filename by
/// name alone, so a rewritten link has to reproduce whichever form the note already used —
/// synthesizing a prefix onto a bare link points it at a directory the image may not live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLinkPath {
    /// `![[image.png]]` — no directory in the link.
    Bare,
    /// `![[conf/media/acheter/image.png]]` — the link carried this directory.
    Qualified(String),
}

impl ImageLinkPath {
    /// Builds the link target for `filename` in the same shape as the original link.
    fn to_link_target(&self, filename: &str) -> String {
        match self {
            Self::Bare => filename.to_string(),
            Self::Qualified(directory) => format!("{directory}{FORWARD_SLASH}{filename}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLink {
    pub matched_text:   String,
    pub position:       usize,
    pub line_number:    usize,
    pub filename:       String,
    pub link_path:      ImageLinkPath,
    pub alt_text:       String,
    pub size_parameter: Option<String>,
    pub state:          ImageLinkState,
    pub link_type:      ImageLinkType,
}

impl ImageLink {
    pub fn new(raw_link: String, line_number: usize, position: usize) -> Result<Self, String> {
        let link_path = extract_link_path(&raw_link);

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
            link_path,
            alt_text: parsed_link.alt_text,
            size_parameter: parsed_link.size_parameter,
            state: ImageLinkState::default(),
            link_type: parsed_link.link_type,
        })
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
                let link_target = self.link_path.to_link_target(new_name);

                match &self.link_type {
                    ImageLinkType::Wiki(rendering) => match rendering {
                        ImageRendering::Embedded => self.size_parameter.as_ref().map_or_else(
                            || {
                                format!(
                                    "{IMAGE_EMBED_MARKER}{OPENING_WIKILINK}{link_target}{CLOSING_WIKILINK}"
                                )
                            },
                            |size| {
                                format!(
                                    "{IMAGE_EMBED_MARKER}{OPENING_WIKILINK}{link_target}{PIPE}{size}{CLOSING_WIKILINK}"
                                )
                            },
                        ),
                        ImageRendering::Linked => {
                            format!("{OPENING_WIKILINK}{link_target}{CLOSING_WIKILINK}")
                        },
                    },
                    ImageLinkType::Markdown(target, rendering) => match (target, rendering) {
                        (ImageLinkTarget::Internal, ImageRendering::Embedded) => {
                            format!(
                                "{IMAGE_LINK_PREFIX}{}{MARKDOWN_LINK_SEPARATOR}{link_target}{CLOSING_PAREN}",
                                self.alt_text
                            )
                        },
                        (ImageLinkTarget::Internal, ImageRendering::Linked) => {
                            format!(
                                "{OPENING_BRACKET}{}{MARKDOWN_LINK_SEPARATOR}{link_target}{CLOSING_PAREN}",
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

#[derive(Debug, Default, Clone, PartialEq, Eq, Deref, DerefMut, IntoIterator)]
pub(crate) struct ImageLinks {
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

fn extract_link_path(matched: &str) -> ImageLinkPath {
    if !matched.contains(FORWARD_SLASH) {
        return ImageLinkPath::Bare;
    }

    let prefix = matched
        .rsplit_once(FORWARD_SLASH)
        .map_or(matched, |(prefix, _)| prefix);

    prefix
        .rfind(|character| matches!(character, OPENING_PAREN | OPENING_BRACKET))
        .map(|index| &prefix[index + 1..])
        .map(|path| path.trim_end_matches(FORWARD_SLASH))
        .filter(|path| !path.is_empty())
        .map_or(ImageLinkPath::Bare, |path| {
            ImageLinkPath::Qualified(path.to_string())
        })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::path::PathBuf;

    use super::ImageLink;
    use super::ImageLinkState;
    use super::ImageLinkTarget;
    use super::ImageLinkType;
    use super::ImageRendering;
    use super::ReplaceableContent;
    use crate::support::IMAGE_REGEX;

    const TEST_IMAGE_LINK_LINE_NUMBER: usize = 1;
    const TEST_IMAGE_LINK_POSITION: usize = 0;

    #[derive(Debug)]
    struct ImageLinkTestCase {
        input:     &'static str,
        filename:  &'static str,
        link_type: ImageLinkType,
    }

    impl ImageLinkTestCase {
        const fn new(
            input: &'static str,
            filename: &'static str,
            link_type: ImageLinkType,
        ) -> Self {
            Self {
                input,
                filename,
                link_type,
            }
        }
    }

    #[test]
    fn test_duplicate_replacement_preserves_link_shape() {
        let cases = [
            // A bare link must stay bare — Obsidian resolves it by filename, and prefixing a
            // directory points it at a folder the keeper may not live in.
            ("![[dup.png]]", "![[keeper.png]]"),
            ("![[dup.png|300]]", "![[keeper.png|300]]"),
            ("[[dup.png]]", "[[keeper.png]]"),
            // A qualified link keeps the directory the note already spelled out.
            (
                "![[conf/media/acheter/dup.png]]",
                "![[conf/media/acheter/keeper.png]]",
            ),
            (
                "![alt](conf/media/dup.png)",
                "![alt](conf/media/keeper.png)",
            ),
        ];

        for (raw_link, expected) in cases {
            let mut image_link = ImageLink::new(
                raw_link.to_string(),
                TEST_IMAGE_LINK_LINE_NUMBER,
                TEST_IMAGE_LINK_POSITION,
            )
            .unwrap();
            image_link.state = ImageLinkState::Duplicate {
                keeper_path: PathBuf::from("conf/media/acheter/keeper.png"),
            };

            assert_eq!(
                image_link.get_replacement(),
                expected,
                "replacement changed the link shape for {raw_link}"
            );
        }
    }

    #[test]
    fn test_image_link_types() {
        let test_cases = [
            // Wikilinks
            ImageLinkTestCase::new(
                "![[image.png]]",
                "image.png",
                ImageLinkType::Wiki(ImageRendering::Embedded),
            ),
            ImageLinkTestCase::new(
                "[[image.jpg]]",
                "image.jpg",
                ImageLinkType::Wiki(ImageRendering::Linked),
            ),
            ImageLinkTestCase::new(
                "![[image.png|alt text]]",
                "image.png",
                ImageLinkType::Wiki(ImageRendering::Embedded),
            ),
            // Markdown Internal Links
            ImageLinkTestCase::new(
                "![alt](image.png)",
                "image.png",
                ImageLinkType::Markdown(ImageLinkTarget::Internal, ImageRendering::Embedded),
            ),
            ImageLinkTestCase::new(
                "[alt](image.jpg)",
                "image.jpg",
                ImageLinkType::Markdown(ImageLinkTarget::Internal, ImageRendering::Linked),
            ),
            // Markdown External Links
            ImageLinkTestCase::new(
                "![alt](https://example.com/image.png)",
                "https://example.com/image.png",
                ImageLinkType::Markdown(ImageLinkTarget::External, ImageRendering::Embedded),
            ),
            ImageLinkTestCase::new(
                "[alt](https://example.com/image.jpg)",
                "https://example.com/image.jpg",
                ImageLinkType::Markdown(ImageLinkTarget::External, ImageRendering::Linked),
            ),
        ];

        for case in &test_cases {
            let captures = IMAGE_REGEX.captures(case.input).unwrap_or_else(|| {
                panic!("Regex failed to match valid image link: {}", case.input)
            });

            let raw_image_link = captures
                .get(0)
                .unwrap_or_else(|| panic!("Failed to get capture group for: {}", case.input))
                .as_str();

            let image_link = ImageLink::new(
                raw_image_link.to_string(),
                TEST_IMAGE_LINK_LINE_NUMBER,
                TEST_IMAGE_LINK_POSITION,
            )
            .unwrap();

            assert_eq!(
                image_link.filename, case.filename,
                "Filename mismatch for input: {}",
                case.input
            );
            assert_eq!(
                image_link.link_type, case.link_type,
                "ImageLinkType mismatch for input: {}",
                case.input
            );
        }
    }
}

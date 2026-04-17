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

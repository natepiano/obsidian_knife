#[derive(Debug, Clone, Copy)]
pub enum AliasExpectation {
    Aliased,
    DirectLink,
}

impl AliasExpectation {
    pub const fn is_alias(self) -> bool { matches!(self, Self::Aliased) }
}

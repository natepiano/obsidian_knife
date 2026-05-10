#[derive(Debug, Clone, Copy)]
pub enum AliasExpectation {
    Aliased,
    DirectLink,
}

impl AliasExpectation {
    pub const fn is_alias(self) -> bool { matches!(self, Self::Aliased) }
}

#[derive(Debug, Clone, Copy)]
pub enum PersistExpectation {
    Persists,
    Unchanged,
}

impl PersistExpectation {
    pub const fn needs_persist(self) -> bool { matches!(self, Self::Persists) }
}

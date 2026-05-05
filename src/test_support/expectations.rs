#[derive(Debug, Clone, Copy)]
pub enum AliasExpectation {
    Alias,
    DirectLink,
}

impl AliasExpectation {
    pub const fn is_alias(self) -> bool { matches!(self, Self::Alias) }
}

#[derive(Debug, Clone, Copy)]
pub enum PersistExpectation {
    Persists,
    DoesNotPersist,
}

impl PersistExpectation {
    pub const fn needs_persist(self) -> bool { matches!(self, Self::Persists) }
}

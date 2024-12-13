use std::ops::Deref;

pub trait EnumFilter {
    type EnumType;
    fn as_enum(&self) -> &Self::EnumType;
}

// Trait that automatically implements filter_by_variant for any type that Derefs to Vec<T>
// mine do because of the vecollect::collection proc_macro which adds a Deref to a Vec<T>
pub trait VecEnumFilter<T>
where
    T: EnumFilter + Clone,
    Self: Deref<Target = Vec<T>> + FromIterator<T>,
{
    /// Filter items directly by matching against a specific variant.
    fn filter_by_variant(&self, variant: T::EnumType) -> Self
    where
        T::EnumType: PartialEq,
    {
        self.iter()
            .filter(|item| item.as_enum() == &variant)
            .cloned()
            .collect()
    }

    fn filter_by_predicate<F>(&self, filter: F) -> Self
    where
        F: Fn(&T::EnumType) -> bool,
    {
        self.iter()
            .filter(|item| filter(item.as_enum()))
            .cloned()
            .collect()
    }
}

// Blanket implementation for any type that Derefs to Vec<T>
// anything has items that implement EnumFilter and Deref to a Vec of those items
// will get filter_by_variant automatically
impl<S, T> VecEnumFilter<T> for S
where
    T: EnumFilter + Clone,
    S: Deref<Target = Vec<T>> + FromIterator<T>,
{
}

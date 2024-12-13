
pub trait EnumFilter {
    type EnumType; // Associated enum type

    fn as_enum(&self) -> &Self::EnumType;
}

// impl<T> Container<T>
// where
//     T: EnumFilter,
// {
//     pub fn filter_by_variant<F>(&self, filter: F) -> Self
//     where
//         F: Fn(&T::EnumType) -> bool,
//     {
//         Self {
//             items: self
//                 .items
//                 .iter()
//                 .filter(|item| filter(item.as_enum()))
//                 .cloned()
//                 .collect(),
//         }
//     }
// }

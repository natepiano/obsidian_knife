/// this macro allows us to persist any extra fields not specifically implemented in
/// a struct you want to deserialize into the yaml frontmatter of a markdown file
///
/// that way if other fields are added, they're not lost
///
/// this makes it so we don't have to remember to manually
/// add the field definitions - which we really couldn't know in advance anyway
#[macro_export]
macro_rules! yaml_frontmatter_struct {
    (
        $(#[$struct_meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field_name:ident : $field_ty:ty
            ),*
            $(,)?
        }
    ) => {
        // Main struct with flattened HashMap
        $(#[$struct_meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field_name: $field_ty,
            )*
            #[serde(flatten)]
            other_fields: std::collections::HashMap<String, serde_yaml::Value>,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    $(
                        $field_name: Default::default(),
                    )*
                    other_fields: std::collections::HashMap::new(),
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $crate::yaml_frontmatter::YamlFrontMatter for $name {}
    };
}

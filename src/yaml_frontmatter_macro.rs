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
        $(#[$struct_meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field_name: $field_ty,
            )*
            #[serde(flatten)]
            pub(crate) other_fields: std::collections::HashMap<String, serde_yaml::Value>
        }

        impl $name {
            /// Creates a new instance with empty other_fields
            pub fn new() -> Self {
                Self {
                    $(
                        $field_name: Default::default(),
                    )*
                    other_fields: std::collections::HashMap::new()
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl crate::yaml_frontmatter::YamlFrontMatter for $name {
            fn other_fields(&self) -> &std::collections::HashMap<String, serde_yaml::Value> {
                &self.other_fields
            }

            fn other_fields_mut(&mut self) -> &mut std::collections::HashMap<String, serde_yaml::Value> {
                &mut self.other_fields
            }
        }
    };
}

// Add test module
#[cfg(test)]
mod macro_tests {
    use serde::{Deserialize, Serialize};

    yaml_frontmatter_struct! {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct TestStruct {
            #[serde(skip_serializing_if = "Option::is_none")]
            field1: Option<String>,
            field2: i32
        }
    }

    #[test]
    fn test_yaml_struct_creation() {
        let test = TestStruct::new();
        assert!(test.other_fields.is_empty());
        assert_eq!(test.field1, None);
        assert_eq!(test.field2, 0);
    }

    #[test]
    fn test_yaml_struct_serialization() {
        let yaml = r#"
            field1: test
            field2: 42
            unknown_field: value
        "#;

        let test: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(test.field1, Some("test".to_string()));
        assert_eq!(test.field2, 42);
        assert_eq!(
            test.other_fields.get("unknown_field"),
            Some(&serde_yaml::Value::String("value".to_string()))
        );
    }
}

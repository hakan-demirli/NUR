use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

id_newtype!(SessionId, "Opaque session identifier minted by the server.");
id_newtype!(MessageId, "Opaque message identifier scoped to a session.");
id_newtype!(PartId, "Opaque part identifier scoped to a message.");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory(pub String);

impl Directory {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Directory {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Directory {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

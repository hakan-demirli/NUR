use serde::{Deserialize, Serialize};

use crate::types::common::{MessageId, PartId};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ToolPart {
    pub id: PartId,
    #[serde(default, rename = "tool")]
    pub tool_name: String,
    #[serde(rename = "messageID", default)]
    pub message_id: Option<MessageId>,
    #[serde(default)]
    pub state: ToolState,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ToolState {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub output: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// reported, BUG1).
    #[serde(default, deserialize_with = "deserialize_tool_error")]
    pub error: Option<String>,
}

/// Why a custom decoder instead of `#[serde(untagged)]` on an enum:
fn deserialize_tool_error<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, MapAccess, Visitor};
    use std::fmt;

    struct ToolErrorVisitor;

    impl<'de> Visitor<'de> for ToolErrorVisitor {
        type Value = Option<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str(
                "a tool error: either a bare string (current opencode wire shape) or \
                 a `{message, name}` object (legacy on-disk shape)",
            )
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D2>(self, d: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            d.deserialize_any(ToolErrorVisitor)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }
        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v))
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut message: Option<String> = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "message" {
                    message = Some(map.next_value::<String>()?);
                } else {
                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
            Ok(Some(message.unwrap_or_default()))
        }
    }

    d.deserialize_any(ToolErrorVisitor)
}

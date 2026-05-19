pub mod client;
pub mod error;
pub mod events;
pub mod types;

pub use client::{Client, ClientConfig};
pub use error::{Error, Result};
pub use events::{EventStream, ServerEvent};
pub use types::{
    common::{Directory, MessageId, PartId, SessionId},
    id::{ascending, ascending_with_timestamp, Prefix},
    message::{Message, MessagePart, MessageRole, MessageWithParts},
    permission::{
        PermissionRepliedProps, PermissionReply, PermissionReplyBody, PermissionRequest,
        PermissionTool,
    },
    provider::{ModelCost, ModelInfo, ProviderInfo, ProviderList},
    question::{
        QuestionInfo, QuestionOption, QuestionRejectedProps, QuestionRepliedProps,
        QuestionReplyBody, QuestionRequest, QuestionTool,
    },
    session::{
        PromptModel, PromptPart, PromptPayload, PromptTextPart, Session, SessionCreateModel,
        SessionCreatePayload, SessionTime,
    },
};

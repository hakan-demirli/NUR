pub mod envelope;
pub mod part;
pub mod parts;

pub use envelope::{Message, MessageRole, MessageTime, MessageWithParts};
pub use part::MessagePart;
pub use parts::{
    compaction::CompactionPart,
    reasoning::ReasoningPart,
    step::StepBoundaryPart,
    text::TextPart,
    tool::{ToolPart, ToolState},
};

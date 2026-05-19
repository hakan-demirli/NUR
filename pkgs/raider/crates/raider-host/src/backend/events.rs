use futures::Stream;
use std::pin::Pin;

use raider_opencode::events::StreamItem;

pub trait EventBackend: Send + Sync + 'static {
    fn events(&self) -> Pin<Box<dyn Stream<Item = StreamItem> + Send>>;
}

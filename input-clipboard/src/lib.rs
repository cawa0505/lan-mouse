mod clipboard;
mod error;
mod origin;

pub use clipboard::{ClipboardSession, Event, MAX_CLIPBOARD_SIZE, watch};
pub use error::{ClipboardCreationError, ClipboardError};
pub use origin::Origin;

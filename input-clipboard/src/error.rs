use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("Wayland connection error: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    #[error("Wayland protocol error: {0}")]
    Wayland(#[from] wayland_client::backend::WaylandError),
    #[error("Wayland dispatch error: {0}")]
    Dispatch(#[from] wayland_client::DispatchError),
    #[error("Wayland global error: {0}")]
    Global(#[from] wayland_client::globals::GlobalError),
    #[error("Wayland bind error: {0}")]
    Bind(#[from] wayland_client::globals::BindError),
    #[error("Protocol unavailable: ext_data_control_manager_v1 not supported")]
    ProtocolUnavailable,
    #[error("No selection present")]
    NoSelection,
    #[error("Unsupported MIME type: {0}")]
    UnsupportedMime(String),
    #[error("Clipboard content exceeds maximum size of {0} bytes")]
    TooLarge(usize),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardCreationError {
    #[error("Failed to create clipboard: {0}")]
    Create(#[from] ClipboardError),
}

//! Customer displays (HW-042, HW-043, SAL-047): the frame the till sends, the 20×2 VFD pole
//! display it becomes, and the service that writes it without ever making the till wait.

pub mod frame;
pub mod service;
pub mod vfd;
mod words;

pub use frame::{DisplayFrame, DisplayLine, DisplayMode, DisplayWeight};
pub use service::DisplayService;
pub use vfd::{VfdCharset, VfdCommands, VfdConfig};

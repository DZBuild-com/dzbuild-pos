//! Checkout scales on a serial port (HW-030, research 05 §4.3): protocols as pure frame parsers,
//! and the service thread that turns readings into `WeightEvent`s (Task 8).

pub mod protocol;
pub mod service;

pub use protocol::{
    Parsed, Protocol, Reading, ReadingStatus, ScaleProtocol, parse_weight, protocol,
};
pub use service::{
    ScaleConfig, ScaleService, ScaleState, Stability, WeightEvent, WeightSink, probe,
};

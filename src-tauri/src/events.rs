//! Typed events from the device threads to the windows (tauri-specta).

use crate::devices::Sinks;
use pos_hw::device::{DeviceKind, DeviceState};
use pos_hw::display::DisplayFrame;
use pos_hw::scale::WeightEvent;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, EventTarget};
use tauri_specta::Event;

/// The scale's reading changed (the main window's scale panel).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct WeightChanged {
    pub reading: WeightEvent,
}

/// A scale, pole display or serial scanner was connected, lost or switched off.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatusChanged {
    pub device: DeviceKind,
    pub state: DeviceState,
}

/// A code from a serial (USB-COM) scanner.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SerialScanned {
    pub code: String,
}

/// What the customer screen shows now.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct CustomerDisplayUpdate {
    pub frame: DisplayFrame,
}

fn to_main<E: Event + Serialize + Clone>(handle: &AppHandle, event: E) {
    if let Err(error) = event.emit_to(handle, EventTarget::webview_window("main")) {
        tracing::debug!(%error, "the main window missed a device event");
    }
}

/// The device threads' sinks: each reading, code and state becomes an event of the main window.
pub fn sinks(handle: &AppHandle) -> Sinks {
    let (weight, scan, status) = (handle.clone(), handle.clone(), handle.clone());
    Sinks {
        weight: Arc::new(move |reading| {
            to_main(
                &weight,
                WeightChanged {
                    reading: reading.clone(),
                },
            );
        }),
        scan: Arc::new(move |code| {
            to_main(
                &scan,
                SerialScanned {
                    code: code.to_owned(),
                },
            );
        }),
        status: Arc::new(move |device, state| {
            to_main(
                &status,
                DeviceStatusChanged {
                    device,
                    state: state.clone(),
                },
            );
        }),
    }
}

/// The frame the till just showed, to the customer screen (when it is open).
pub fn to_customer_display(handle: &AppHandle, frame: DisplayFrame) {
    let event = CustomerDisplayUpdate { frame };
    let target = EventTarget::webview_window(crate::customer_display::LABEL);
    if let Err(error) = event.emit_to(handle, target) {
        tracing::debug!(%error, "no customer screen to update");
    }
}

//! Hardware failures with stable codes. `src-tauri` turns them into `AppError::Hardware { code }`
//! (or `AppError::Validation` for [`HwError::Invalid`]); the UI translates `errors:<code>`.

/// Every code a [`HwError`] carries. `src/locales/*/errors.json` translates each one: the
/// `src-tauri` contract test `every_hardware_code_has_a_message_in_every_language` checks it.
pub const KNOWN_HW_CODES: &[&str] = &[
    "printer_not_configured",
    "printer_not_found",
    "printer_offline",
    "printer_io_error",
    "printer_timeout",
    "printer_setting_invalid",
    "port_unavailable",
    "printer_raw_unsupported",
    "logo_unreadable",
    "render_failed",
    "hardware_unsupported",
    "hardware_config_invalid",
    "barcode_invalid",
    "hardware_setting_invalid",
    "scale_not_configured",
    "scale_not_responding",
    "scale_unstable",
    "scale_command_unsupported",
    "scale_overload",
    "display_not_configured",
    "collector_file_invalid",
    "label_printer_not_configured",
    "printer_driver_only",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HwError {
    #[error("no receipt printer is configured")]
    NotConfigured,
    #[error("printer {0} was not found")]
    PrinterNotFound(String),
    #[error("printer is offline: {0}")]
    Offline(String),
    #[error("printer i/o error: {0}")]
    Io(String),
    #[error("the printer did not answer in time")]
    Timeout,
    #[error("invalid printer setting {field}")]
    Invalid { field: String },
    #[error("port {0} is not available")]
    PortUnavailable(String),
    #[error("the Windows driver refuses raw data: {0}")]
    RawUnsupported(String),
    #[error("the logo cannot be read: {0}")]
    Logo(String),
    #[error("rendering failed: {0}")]
    Render(String),
    #[error("not supported on this system")]
    Unsupported,
    #[error("the hardware settings file is invalid: {0}")]
    Config(String),
    #[error("barcode: {0}")]
    Barcode(String),
    #[error("invalid hardware setting {field}")]
    Setting { field: String },
    #[error("no scale is configured")]
    ScaleNotConfigured,
    #[error("the scale does not answer")]
    ScaleNotResponding,
    #[error("the weight is not stable")]
    ScaleUnstable,
    #[error("this scale does not take that command")]
    ScaleCommandUnsupported,
    #[error("the scale is overloaded")]
    ScaleOverload,
    #[error("no pole display is configured")]
    DisplayNotConfigured,
    #[error("collector file: {0}")]
    Collector(String),
    #[error("no label printer is configured")]
    LabelPrinterNotConfigured,
    /// HW-002: a printer set to print pages through its Windows driver never gets ESC/POS; the
    /// app prints the page on `printer_name`, `paper_width_mm` wide, through M3's print host.
    #[error("printer {printer_name} prints pages only")]
    DriverOnly {
        printer_name: String,
        paper_width_mm: u16,
    },
}

impl HwError {
    pub fn invalid(field: &str) -> HwError {
        HwError::Invalid {
            field: field.to_owned(),
        }
    }

    pub fn setting(field: &str) -> HwError {
        HwError::Setting {
            field: field.to_owned(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            HwError::NotConfigured => "printer_not_configured",
            HwError::PrinterNotFound(_) => "printer_not_found",
            HwError::Offline(_) => "printer_offline",
            HwError::Io(_) => "printer_io_error",
            HwError::Timeout => "printer_timeout",
            HwError::Invalid { .. } => "printer_setting_invalid",
            HwError::PortUnavailable(_) => "port_unavailable",
            HwError::RawUnsupported(_) => "printer_raw_unsupported",
            HwError::Logo(_) => "logo_unreadable",
            HwError::Render(_) => "render_failed",
            HwError::Unsupported => "hardware_unsupported",
            HwError::Config(_) => "hardware_config_invalid",
            HwError::Barcode(_) => "barcode_invalid",
            HwError::Setting { .. } => "hardware_setting_invalid",
            HwError::ScaleNotConfigured => "scale_not_configured",
            HwError::ScaleNotResponding => "scale_not_responding",
            HwError::ScaleUnstable => "scale_unstable",
            HwError::ScaleCommandUnsupported => "scale_command_unsupported",
            HwError::ScaleOverload => "scale_overload",
            HwError::DisplayNotConfigured => "display_not_configured",
            HwError::Collector(_) => "collector_file_invalid",
            HwError::LabelPrinterNotConfigured => "label_printer_not_configured",
            HwError::DriverOnly { .. } => "printer_driver_only",
        }
    }

    /// The same error with `prefix.` before its field (a nested setting).
    pub fn within(self, prefix: &str) -> HwError {
        match self {
            HwError::Invalid { field } => HwError::Invalid {
                field: format!("{prefix}.{field}"),
            },
            HwError::Setting { field } => HwError::Setting {
                field: format!("{prefix}.{field}"),
            },
            other => other,
        }
    }

    /// The settings field at fault (camelCase path), for [`HwError::Invalid`] and
    /// [`HwError::Setting`].
    pub fn field(&self) -> Option<&str> {
        match self {
            HwError::Invalid { field } | HwError::Setting { field } => Some(field),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_has_a_known_code() {
        let all = [
            HwError::NotConfigured,
            HwError::PrinterNotFound("XP-80".into()),
            HwError::Offline("refused".into()),
            HwError::Io("broken pipe".into()),
            HwError::Timeout,
            HwError::Invalid {
                field: "transport.host".into(),
            },
            HwError::PortUnavailable("COM7".into()),
            HwError::RawUnsupported("XPS driver".into()),
            HwError::Logo("not an image".into()),
            HwError::Render("font".into()),
            HwError::Unsupported,
            HwError::Config("trailing comma".into()),
            HwError::Barcode("check digit".into()),
            HwError::setting("serial.baud"),
            HwError::ScaleNotConfigured,
            HwError::ScaleNotResponding,
            HwError::ScaleUnstable,
            HwError::ScaleCommandUnsupported,
            HwError::ScaleOverload,
            HwError::DisplayNotConfigured,
            HwError::Collector("empty".into()),
            HwError::LabelPrinterNotConfigured,
            HwError::DriverOnly {
                printer_name: "HP LaserJet".into(),
                paper_width_mm: 80,
            },
        ];
        let mut codes: Vec<&str> = all.iter().map(HwError::code).collect();
        codes.sort_unstable();
        let mut known = KNOWN_HW_CODES.to_vec();
        known.sort_unstable();
        assert_eq!(codes, known, "one code per variant, each listed once");
        assert_eq!(
            HwError::Invalid {
                field: "profile.dots".into()
            }
            .field(),
            Some("profile.dots")
        );
        assert_eq!(HwError::setting("serial.port").field(), Some("serial.port"));
        assert_eq!(HwError::Timeout.field(), None);
    }

    #[test]
    fn a_nested_setting_names_its_whole_path() {
        assert_eq!(
            HwError::setting("serial.port").within("scale").field(),
            Some("scale.serial.port")
        );
        assert_eq!(
            HwError::invalid("transport.path")
                .within("labelPrinters.0")
                .field(),
            Some("labelPrinters.0.transport.path")
        );
        assert_eq!(
            HwError::invalid("id").within("labelPrinters.0").code(),
            "printer_setting_invalid",
            "the code stays"
        );
        assert_eq!(HwError::Timeout.within("scale"), HwError::Timeout);
    }
}

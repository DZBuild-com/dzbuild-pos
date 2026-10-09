//! The two commands of the hidden print window (capability `print-host`). Any other window is
//! refused: the main window never reads a print job, the print host never reaches the rest.

use crate::print_host::{HostJob, PRINT_HOST, PrintHost, RenderResult};
use pos_core::error::AppError;
use std::sync::Arc;
use tauri::State;

fn only_the_print_host(label: &str) -> Result<(), AppError> {
    if label == PRINT_HOST {
        Ok(())
    } else {
        Err(AppError::auth("insufficient_permission"))
    }
}

pub(crate) fn next(host: &PrintHost, label: &str) -> Result<Option<HostJob>, AppError> {
    only_the_print_host(label)?;
    Ok(host.channel().take())
}

pub(crate) fn rendered(
    host: &PrintHost,
    label: &str,
    job_id: &str,
    result: RenderResult,
) -> Result<(), AppError> {
    only_the_print_host(label)?;
    host.channel().rendered(job_id, result);
    Ok(())
}

/// The job waiting for the print host, taken (the next call answers `None`).
#[tauri::command]
#[specta::specta]
pub async fn print_host_next(
    window: tauri::Window,
    host: State<'_, Arc<PrintHost>>,
) -> Result<Option<HostJob>, AppError> {
    next(&host, window.label())
}

/// The print host has laid out `job_id` (or failed to): the waiting print or PDF goes on.
#[tauri::command]
#[specta::specta]
pub async fn print_host_rendered(
    window: tauri::Window,
    host: State<'_, Arc<PrintHost>>,
    job_id: String,
    result: RenderResult,
) -> Result<(), AppError> {
    rendered(&host, window.label(), &job_id, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print_host::{HostChannel, PrintHost, RenderResult};

    struct Nothing;
    impl crate::print_host::PrintSurface for Nothing {
        fn render(
            &self,
            _: crate::print_host::HostJob,
        ) -> Result<crate::print_host::Rendered, AppError> {
            Err(AppError::conflict("print_host_unavailable"))
        }
        fn print(
            &self,
            _: &crate::print_host::PageSetup,
            _: Option<&str>,
            _: u32,
        ) -> Result<crate::print_host::PrintStatus, AppError> {
            Err(AppError::conflict("print_host_unavailable"))
        }
        fn pdf(
            &self,
            _: &crate::print_host::PageSetup,
            _: &std::path::Path,
        ) -> Result<(), AppError> {
            Err(AppError::conflict("print_host_unavailable"))
        }
        fn close_if_idle(&self, _: std::time::Duration) {}
    }

    #[test]
    fn print_host_commands_refuse_other_windows() {
        let host = PrintHost::new(
            std::sync::Arc::new(Nothing),
            std::sync::Arc::new(HostChannel::new()),
        );
        assert_eq!(
            next(&host, "main").unwrap_err().code(),
            "insufficient_permission"
        );
        let result = RenderResult {
            ok: true,
            error: None,
            height_px: 0,
        };
        assert_eq!(
            rendered(&host, "main", "j1", result.clone())
                .unwrap_err()
                .code(),
            "insufficient_permission"
        );
        assert_eq!(next(&host, crate::print_host::PRINT_HOST).unwrap(), None);
        rendered(&host, crate::print_host::PRINT_HOST, "unknown", result).unwrap();
    }
}

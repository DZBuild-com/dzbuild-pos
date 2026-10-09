//! RAW jobs through the Windows spooler (`OpenPrinterW` / `StartDocPrinterW` / `WritePrinter`).
//!
//! The spooler accepts a job for a printer that is off, unplugged or out of paper and keeps it
//! queued: a receipt would count as printed, and print later next to its DUPLICATA. So the
//! printer's state and its queue are read before the job (`GetPrinterW`, `EnumJobsW`), and the
//! job is watched for about two seconds after it is sent (`GetJobW`); a job that fails is taken
//! off the queue (`SetJobW` `JOB_CONTROL_DELETE`) and the print reported as offline.
#![allow(unsafe_code)] // FFI to winspool.drv; each unsafe block states its contract.

use super::Driver;
use crate::error::HwError;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DOC_INFO_1W, EndDocPrinter, EndPagePrinter, EnumJobsW, GetJobW, GetPrinterW,
    JOB_CONTROL_DELETE, JOB_INFO_1W, JOB_STATUS_BLOCKED_DEVQ, JOB_STATUS_COMPLETE,
    JOB_STATUS_DELETED, JOB_STATUS_ERROR, JOB_STATUS_OFFLINE, JOB_STATUS_PAPEROUT,
    JOB_STATUS_PRINTED, JOB_STATUS_USER_INTERVENTION, OpenPrinterW, PRINTER_ATTRIBUTE_WORK_OFFLINE,
    PRINTER_HANDLE, PRINTER_INFO_2W, PRINTER_STATUS_DOOR_OPEN, PRINTER_STATUS_ERROR,
    PRINTER_STATUS_NOT_AVAILABLE, PRINTER_STATUS_OFFLINE, PRINTER_STATUS_PAPER_OUT,
    PRINTER_STATUS_PAPER_PROBLEM, PRINTER_STATUS_PAUSED, PRINTER_STATUS_USER_INTERVENTION, SetJobW,
    StartDocPrinterW, StartPagePrinter, WritePrinter,
};
use windows::core::{PCWSTR, PWSTR};

/// ERROR_INVALID_DATATYPE: an XPS-class driver that takes no RAW data.
const INVALID_DATATYPE: i32 = 1804;

/// How long a sent job is watched for an error before it counts as printed.
const WATCH_FOR: Duration = Duration::from_secs(2);
const WATCH_EVERY: Duration = Duration::from_millis(100);

/// Printer states that keep a job from printing now.
const PRINTER_FAULTS: [(u32, &str); 8] = [
    (PRINTER_STATUS_OFFLINE, "offline"),
    (PRINTER_STATUS_ERROR, "error"),
    (PRINTER_STATUS_PAPER_OUT, "paper out"),
    (PRINTER_STATUS_PAPER_PROBLEM, "paper problem"),
    (PRINTER_STATUS_NOT_AVAILABLE, "not available"),
    (PRINTER_STATUS_USER_INTERVENTION, "needs attention"),
    (PRINTER_STATUS_DOOR_OPEN, "door open"),
    (PRINTER_STATUS_PAUSED, "paused"),
];

/// Job states of a queue that is stuck, or of a job that did not print.
const JOB_FAULTS: u32 = JOB_STATUS_ERROR
    | JOB_STATUS_OFFLINE
    | JOB_STATUS_PAPEROUT
    | JOB_STATUS_BLOCKED_DEVQ
    | JOB_STATUS_USER_INTERVENTION;

/// Job states of a job that is through.
const JOB_DONE: u32 = JOB_STATUS_PRINTED | JOB_STATUS_COMPLETE | JOB_STATUS_DELETED;

/// Why the printer cannot print now, from `PRINTER_INFO_2W.Attributes` and `.Status`.
fn printer_fault(attributes: u32, status: u32) -> Option<&'static str> {
    if attributes & PRINTER_ATTRIBUTE_WORK_OFFLINE != 0 {
        return Some("set to work offline");
    }
    PRINTER_FAULTS
        .iter()
        .find(|(flag, _)| status & flag != 0)
        .map(|(_, reason)| *reason)
}

fn job_failed(status: u32) -> bool {
    status & JOB_FAULTS != 0
}

fn job_done(status: u32) -> bool {
    status & JOB_DONE != 0
}

pub(super) struct WinspoolDriver {
    name: String,
}

impl WinspoolDriver {
    pub(super) fn new(name: &str) -> WinspoolDriver {
        WinspoolDriver {
            name: name.trim().to_owned(),
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Closes the printer handle exactly once.
struct Handle(PRINTER_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenPrinterW and is closed only here.
        let _ = unsafe { ClosePrinter(self.0) };
    }
}

/// A buffer of `bytes` bytes (at least one `T`) aligned for the spooler's records (u64 words).
fn words_for<T>(bytes: u32) -> Vec<u64> {
    let bytes = (bytes as usize).max(std::mem::size_of::<T>());
    vec![0_u64; bytes.div_ceil(8)]
}

/// The bytes of `words`, for the spooler to write into.
fn as_bytes(words: &mut [u64]) -> &mut [u8] {
    let len = words.len() * 8;
    // SAFETY: `words` is a live, exclusively borrowed allocation of `len` bytes; u8 has no
    // alignment or validity requirement.
    unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), len) }
}

/// `(Attributes, Status, cJobs)` of the printer; `None` when the spooler will not say.
fn printer_state(handle: &Handle) -> Option<(u32, u32, u32)> {
    let mut needed = 0_u32;
    // SAFETY: a size query without a buffer; `needed` is a valid out pointer.
    let _ = unsafe { GetPrinterW(handle.0, 2, None, &mut needed) };
    if needed == 0 {
        return None;
    }
    let mut words = words_for::<PRINTER_INFO_2W>(needed);
    // SAFETY: the buffer is writable for its length; `needed` is a valid out pointer.
    unsafe { GetPrinterW(handle.0, 2, Some(as_bytes(&mut words)), &mut needed) }.ok()?;
    // SAFETY: GetPrinterW wrote one PRINTER_INFO_2W at the start of the u64-aligned buffer,
    // which is at least `needed` bytes long; only its plain integer fields are read.
    let info = unsafe { std::ptr::read(words.as_ptr().cast::<PRINTER_INFO_2W>()) };
    Some((info.Attributes, info.Status, info.cJobs))
}

/// The status of every job queued on the printer (at most `count`).
fn queued_jobs(handle: &Handle, count: u32) -> Vec<u32> {
    if count == 0 {
        return Vec::new();
    }
    let (mut needed, mut returned) = (0_u32, 0_u32);
    // SAFETY: a size query without a buffer; out pointers are valid.
    let _ = unsafe { EnumJobsW(handle.0, 0, count, 1, None, &mut needed, &mut returned) };
    if needed == 0 {
        return Vec::new();
    }
    let mut words = words_for::<JOB_INFO_1W>(needed);
    // SAFETY: the buffer is writable for its length; out pointers are valid.
    let listed = unsafe {
        EnumJobsW(
            handle.0,
            0,
            count,
            1,
            Some(as_bytes(&mut words)),
            &mut needed,
            &mut returned,
        )
    };
    if listed.is_err() {
        return Vec::new();
    }
    let jobs = words.as_ptr().cast::<JOB_INFO_1W>();
    (0..returned as usize)
        // SAFETY: EnumJobsW wrote `returned` JOB_INFO_1W records at the start of the u64-aligned
        // buffer; only their plain integer fields are read.
        .map(|i| unsafe { std::ptr::read(jobs.add(i)) }.Status)
        .collect()
}

/// The status of job `id`; `None` once it left the queue (printed) or cannot be read.
fn job_status(handle: &Handle, id: u32) -> Option<u32> {
    let mut needed = 0_u32;
    // SAFETY: a size query without a buffer; `needed` is a valid out pointer.
    let _ = unsafe { GetJobW(handle.0, id, 1, None, &mut needed) };
    if needed == 0 {
        return None;
    }
    let mut words = words_for::<JOB_INFO_1W>(needed);
    // SAFETY: the buffer is writable for its length; `needed` is a valid out pointer.
    let ok = unsafe { GetJobW(handle.0, id, 1, Some(as_bytes(&mut words)), &mut needed) };
    if !ok.as_bool() {
        return None;
    }
    // SAFETY: GetJobW wrote one JOB_INFO_1W at the start of the u64-aligned buffer; only its
    // plain integer fields are read.
    Some(unsafe { std::ptr::read(words.as_ptr().cast::<JOB_INFO_1W>()) }.Status)
}

impl WinspoolDriver {
    /// Refuses a printer that cannot print now, or whose queue is stuck on an earlier job.
    fn check_ready(&self, handle: &Handle) -> Result<(), HwError> {
        let Some((attributes, status, jobs)) = printer_state(handle) else {
            return Ok(());
        };
        if let Some(reason) = printer_fault(attributes, status) {
            return Err(HwError::Offline(format!("{}: {reason}", self.name)));
        }
        if queued_jobs(handle, jobs).into_iter().any(job_failed) {
            return Err(HwError::Offline(format!(
                "{}: an earlier job is stuck in the queue",
                self.name
            )));
        }
        Ok(())
    }

    /// Watches job `id` for [`WATCH_FOR`]: a job that fails (or a printer that goes offline
    /// under it) is deleted, so it never prints later, and the print is reported offline.
    fn watch(&self, handle: &Handle, id: u32) -> Result<(), HwError> {
        let started = Instant::now();
        loop {
            let Some(status) = job_status(handle, id) else {
                return Ok(());
            };
            if job_done(status) {
                return Ok(());
            }
            let printer = printer_state(handle).and_then(|(a, s, _)| printer_fault(a, s));
            if job_failed(status) || printer.is_some() {
                // SAFETY: the handle is open; no job record is passed with JOB_CONTROL_DELETE.
                let _ = unsafe { SetJobW(handle.0, id, 0, None, JOB_CONTROL_DELETE) };
                return Err(HwError::Offline(format!(
                    "{}: {}",
                    self.name,
                    printer.unwrap_or("the job did not print")
                )));
            }
            if started.elapsed() >= WATCH_FOR {
                return Ok(());
            }
            std::thread::sleep(WATCH_EVERY);
        }
    }
}

/// `(Status, Attributes)` of printer `name` through `GetPrinterW` level 2 (HW-008).
pub(super) fn status(name: &str) -> Result<(u32, u32), HwError> {
    let name = name.trim();
    let wide_name = wide(name);
    let mut raw = PRINTER_HANDLE::default();
    // SAFETY: `wide_name` is a NUL-terminated UTF-16 buffer alive for the call; `raw` is a valid
    // out pointer.
    unsafe { OpenPrinterW(PCWSTR(wide_name.as_ptr()), &mut raw, None) }
        .map_err(|_| HwError::PrinterNotFound(name.to_owned()))?;
    let handle = Handle(raw);
    let (attributes, status, _jobs) = printer_state(&handle)
        .ok_or_else(|| HwError::Io(format!("{name}: the spooler gives no status")))?;
    Ok((status, attributes))
}

impl Driver for WinspoolDriver {
    fn send(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        let len =
            u32::try_from(bytes.len()).map_err(|_| HwError::Render("job too large".into()))?;
        let name = wide(&self.name);
        let mut raw = PRINTER_HANDLE::default();
        // SAFETY: `name` is a NUL-terminated UTF-16 buffer alive for the call; `raw` is a valid
        // out pointer.
        unsafe { OpenPrinterW(PCWSTR(name.as_ptr()), &mut raw, None) }
            .map_err(|_| HwError::PrinterNotFound(self.name.clone()))?;
        let handle = Handle(raw);
        self.check_ready(&handle)?;
        let mut doc_name = wide("DZBuild POS");
        let mut datatype = wide("RAW");
        let info = DOC_INFO_1W {
            pDocName: PWSTR(doc_name.as_mut_ptr()),
            pOutputFile: PWSTR::null(),
            pDatatype: PWSTR(datatype.as_mut_ptr()),
        };
        // SAFETY: the handle is open; `info` points at buffers alive for the call.
        let job = unsafe { StartDocPrinterW(handle.0, 1, &info) };
        if job == 0 {
            let error = std::io::Error::last_os_error();
            return Err(if error.raw_os_error() == Some(INVALID_DATATYPE) {
                HwError::RawUnsupported(self.name.clone())
            } else {
                HwError::Offline(error.to_string())
            });
        }
        // SAFETY: a document is started on this open handle.
        let page = unsafe { StartPagePrinter(handle.0) }.as_bool();
        let mut written = 0_u32;
        // SAFETY: `bytes` is readable for `len` bytes; `written` is a valid out pointer.
        let wrote = page
            && unsafe { WritePrinter(handle.0, bytes.as_ptr().cast(), len, &mut written) }
                .as_bool();
        let error = std::io::Error::last_os_error();
        if page {
            // SAFETY: a page was started on this handle.
            let _ = unsafe { EndPagePrinter(handle.0) };
        }
        // SAFETY: a document was started on this handle.
        let _ = unsafe { EndDocPrinter(handle.0) };
        if !(wrote && written == len) {
            // SAFETY: the handle is open; a partial job must not print later.
            let _ = unsafe { SetJobW(handle.0, job, 0, None, JOB_CONTROL_DELETE) };
            return Err(HwError::Io(error.to_string()));
        }
        self.watch(&handle, job)
    }

    fn describe(&self) -> String {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_printer_that_cannot_print_now_is_refused() {
        assert_eq!(printer_fault(0, 0), None);
        assert_eq!(
            printer_fault(PRINTER_ATTRIBUTE_WORK_OFFLINE, 0),
            Some("set to work offline")
        );
        for (flag, reason) in PRINTER_FAULTS {
            assert_eq!(printer_fault(0, flag), Some(reason));
        }
        // Busy, printing or warming up is not a fault.
        let busy = 0x0000_0200 | 0x0000_0400 | 0x0001_0000;
        assert_eq!(printer_fault(0, busy), None);
    }

    #[test]
    fn stuck_and_finished_jobs_are_told_apart() {
        for flag in [
            JOB_STATUS_ERROR,
            JOB_STATUS_OFFLINE,
            JOB_STATUS_PAPEROUT,
            JOB_STATUS_BLOCKED_DEVQ,
            JOB_STATUS_USER_INTERVENTION,
        ] {
            assert!(job_failed(flag) && !job_done(flag));
        }
        for flag in [JOB_STATUS_PRINTED, JOB_STATUS_COMPLETE, JOB_STATUS_DELETED] {
            assert!(job_done(flag) && !job_failed(flag));
        }
        // Spooling (8) or printing (16): still going.
        assert!(!job_failed(8 | 16) && !job_done(8 | 16));
    }
}

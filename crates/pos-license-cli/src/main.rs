//! `dzpos-license` (see the library's docs and docs/release/licensing.md).

use pos_license_cli::{CliError, Env};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

struct Console {
    out: std::io::Stdout,
}

impl Env for Console {
    fn passphrase(&mut self, prompt: &str) -> Result<String, CliError> {
        if let Some(value) = std::env::var_os("DZPOS_LICENSE_PASSPHRASE") {
            return value
                .into_string()
                .map_err(|_| CliError::new("DZPOS_LICENSE_PASSPHRASE is not text"));
        }
        rpassword::prompt_password(prompt)
            .map_err(|e| CliError::new(format!("the passphrase could not be read: {e}")))
    }

    fn now(&self) -> i64 {
        pos_core::time::now_ms()
    }

    fn out(&mut self) -> &mut dyn Write {
        &mut self.out
    }

    fn default_keys_dir(&self) -> Option<PathBuf> {
        std::env::var_os("DZPOS_LICENSE_KEYS")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|home| PathBuf::from(home).join("DZBuild POS licence key"))
            })
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut console = Console {
        out: std::io::stdout(),
    };
    match pos_license_cli::run(&argv, &mut console) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dzpos-license: {error}");
            ExitCode::from(2)
        }
    }
}

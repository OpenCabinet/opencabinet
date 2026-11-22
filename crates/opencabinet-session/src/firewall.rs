// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Windows firewall, which blocks who joins a session unless it lets
//! Review in.
//!
//! Review never changes the firewall by itself: the host presses a button,
//! and Windows asks for an administrator's permission before adding one
//! rule that lets in this program only, on private and work networks.
//! Elsewhere there is nothing to check.

use std::path::Path;

/// The name of Review's rule in the Windows firewall.
pub const RULE: &str = "OpenCabinet Review sessions";

/// What stands between a host and who joins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Firewall {
    /// Whether Review's rule exists for this program; `None` when it
    /// cannot be told, or there is no Windows firewall.
    pub allowed: Option<bool>,
    /// Whether Windows treats the network as public, where it blocks
    /// what the rule allows on private networks.
    pub public: bool,
}

impl Firewall {
    /// A message for the host, when something may keep people out.
    #[must_use]
    pub fn advice(self) -> Option<&'static str> {
        if self.public {
            Some(
                "Windows treats this network as public and blocks who joins. If it is your office or home, set it to private: Settings › Network & internet › your connection › Private network.",
            )
        } else if self.allowed == Some(false) {
            Some("The Windows firewall may block who joins: allow Review through it.")
        } else {
            None
        }
    }
}

/// Looks at the firewall for `program`. It takes a moment: run it away
/// from the window.
#[must_use]
pub fn check(program: &Path) -> Firewall {
    imp::check(program)
}

/// Asks Windows to let `program` in, which shows Windows' own permission
/// prompt.
///
/// # Errors
/// A message for the user when it was not allowed.
pub fn allow(program: &Path) -> Result<(), String> {
    imp::allow(program)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt as _;
    use std::path::Path;
    use std::process::{Command, Output};

    use super::{Firewall, RULE};

    /// Keeps a console window from flashing.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn powershell(script: &str) -> std::io::Result<Output> {
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    }

    /// `text` between single quotes for PowerShell.
    fn quoted(text: &str) -> String {
        format!("'{}'", text.replace('\'', "''"))
    }

    pub fn check(program: &Path) -> Firewall {
        let program = program.to_string_lossy();
        // Exits 0 when a rule of ours names this program; prints
        // "Public" when a connected network is public.
        let script = format!(
            "$app = Get-NetFirewallRule -DisplayName {rule} -ErrorAction SilentlyContinue | Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -eq {program} }}; \
             $public = Get-NetConnectionProfile -ErrorAction SilentlyContinue | Where-Object {{ $_.NetworkCategory -eq 'Public' }}; \
             if ($public) {{ 'Public' }}; if ($app) {{ exit 0 }} else {{ exit 3 }}",
            rule = quoted(RULE),
            program = quoted(&program),
        );
        match powershell(&script) {
            Ok(output) => Firewall {
                allowed: match output.status.code() {
                    Some(0) => Some(true),
                    Some(3) => Some(false),
                    _ => None,
                },
                public: String::from_utf8_lossy(&output.stdout).contains("Public"),
            },
            Err(_) => Firewall::default(),
        }
    }

    pub fn allow(program: &Path) -> Result<(), String> {
        let program = program.to_string_lossy();
        let arguments = format!(
            "advfirewall firewall add rule name=\"{RULE}\" dir=in action=allow program=\"{program}\" enable=yes profile=private,domain"
        );
        // Start-Process -Verb RunAs shows Windows' permission prompt.
        let script = format!(
            "try {{ $p = Start-Process -FilePath netsh.exe -ArgumentList {arguments} -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 5 }}",
            arguments = quoted(&arguments),
        );
        let output = powershell(&script).map_err(|e| format!("Cannot ask Windows: {e}"))?;
        match output.status.code() {
            Some(0) => Ok(()),
            Some(5) => Err("Windows did not give permission to change the firewall.".into()),
            _ => Err("Windows could not add the firewall rule.".into()),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    use super::Firewall;

    pub fn check(_program: &Path) -> Firewall {
        Firewall::default()
    }

    pub fn allow(_program: &Path) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advice_names_what_keeps_people_out() {
        assert_eq!(Firewall::default().advice(), None);
        let blocked = Firewall {
            allowed: Some(false),
            public: false,
        };
        assert!(blocked.advice().unwrap().contains("allow Review"));
        let public = Firewall {
            allowed: Some(true),
            public: true,
        };
        assert!(public.advice().unwrap().contains("public"));
    }

    #[test]
    fn checking_answers_without_changing_anything() {
        // Only reads; on Windows it asks PowerShell, elsewhere nothing.
        let found = check(Path::new("C:/no/such/program.exe"));
        assert_ne!(found.allowed, Some(true));
    }
}

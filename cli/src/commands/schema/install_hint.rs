// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Platform-aware terraform install hint. Surfaced when `terrashift schema
//! update` discovers `terraform` is not on PATH.
//!
//! Per RFC §4.2: "If missing, print a platform-aware install hint (brew on
//! macOS, apt on Debian/Ubuntu, choco on Windows, plus the terraform.io
//! download link) and exit non-zero."

/// Render the install hint for the current platform. The string is
/// pre-formatted so it can be `eprintln!`d directly.
pub fn for_current_platform() -> String {
    let mut out = String::new();
    out.push_str("hint: install terraform >= 1.0\n");

    #[cfg(target_os = "macos")]
    {
        out.push_str("\n  macOS (Homebrew):\n");
        out.push_str("    brew tap hashicorp/tap\n");
        out.push_str("    brew install hashicorp/tap/terraform\n");
    }

    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    {
        out.push_str("\n  Debian/Ubuntu:\n");
        out.push_str("    wget -O- https://apt.releases.hashicorp.com/gpg | \\\n");
        out.push_str(
            "      sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg\n",
        );
        out.push_str("    echo \"deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] \\\n");
        out.push_str("      https://apt.releases.hashicorp.com $(lsb_release -cs) main\" | \\\n");
        out.push_str("      sudo tee /etc/apt/sources.list.d/hashicorp.list\n");
        out.push_str("    sudo apt update && sudo apt install terraform\n");
    }

    #[cfg(target_os = "windows")]
    {
        out.push_str("\n  Windows (Chocolatey):\n");
        out.push_str("    choco install terraform\n");
        out.push_str("\n  Windows (winget):\n");
        out.push_str("    winget install --id Hashicorp.Terraform\n");
    }

    out.push_str("\n  Or download a release binary:\n");
    out.push_str("    https://developer.hashicorp.com/terraform/install\n");
    out.push_str("\nAfter install, re-run `terrashift schema update`.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_mentions_terraform_io_for_every_platform() {
        let hint = for_current_platform();
        assert!(hint.contains("terraform"));
        assert!(hint.contains("hashicorp.com") || hint.contains("hashicorp/tap"));
        assert!(hint.contains("https://"));
    }

    #[test]
    fn hint_is_actionable_not_decorative() {
        let hint = for_current_platform();
        // Must contain at least one shell command line — every supported
        // platform branch above ends in an actionable command.
        let has_command = hint.contains("brew install")
            || hint.contains("apt install")
            || hint.contains("choco install")
            || hint.contains("winget install");
        assert!(has_command, "install hint must include a runnable command");
    }
}

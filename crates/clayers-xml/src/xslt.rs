use std::fs;
use std::process::Command;

use crate::error::Error;

/// Transform XML using a set of XSLT files.
///
/// The first entry in `xslt_files` is the main stylesheet (entry point);
/// the rest are written alongside it so that `xsl:import href="foo.xslt"`
/// resolves relative to the same directory.
///
/// Returns the transformed output as a string.
///
/// # Errors
///
/// Returns `Error::Xslt` if Saxon is not found, fails to run, or
/// produces non-zero exit status.
pub fn transform(xml: &str, xslt_files: &[(&str, &str)]) -> Result<String, Error> {
    let tmp_dir = tempfile::tempdir().map_err(|e| Error::Xslt(format!("tempdir: {e}")))?;

    // Write all XSLT files into the temp directory.
    for (name, content) in xslt_files {
        fs::write(tmp_dir.path().join(name), content)?;
    }

    // Write the XML input.
    let xml_path = tmp_dir.path().join("input.xml");
    fs::write(&xml_path, xml)?;

    // Determine the main stylesheet (first entry).
    let main_xslt = xslt_files
        .first()
        .map(|(name, _)| name)
        .ok_or_else(|| Error::Xslt("no XSLT files provided".into()))?;
    let xslt_path = tmp_dir.path().join(main_xslt);

    // Find saxon.
    let saxon = find_saxon()?;

    // Run saxon.
    let output = Command::new(&saxon)
        .arg(format!("-s:{}", xml_path.display()))
        .arg(format!("-xsl:{}", xslt_path.display()))
        .output()
        .map_err(|e| Error::Xslt(format!("failed to run saxon: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Xslt(format!("saxon failed: {stderr}")));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| Error::Xslt(format!("saxon output is not UTF-8: {e}")))
}

fn find_saxon() -> Result<String, Error> {
    // Check PATH for `saxon`.
    let check = Command::new("which").arg("saxon").output();
    if let Ok(output) = check
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return Err(Error::Xslt(install_instructions()));
        }
        return Ok(path);
    }

    Err(Error::Xslt(install_instructions()))
}

fn install_instructions() -> String {
    let os = std::env::consts::OS;
    let mut msg = String::from("saxon not found in PATH.\n\nInstall instructions:\n");

    match os {
        "macos" => {
            msg.push_str("  brew install saxon\n");
        }
        "linux" => {
            msg.push_str(
                "  Download Saxon-HE from:\n  \
                 https://github.com/Saxonica/Saxon-HE/releases\n  \
                 Then add the `saxon` wrapper script to your PATH.\n",
            );
        }
        "windows" => {
            msg.push_str(
                "  Download Saxon-HE from:\n  \
                 https://github.com/Saxonica/Saxon-HE/releases\n  \
                 Then add saxon.exe to your PATH.\n",
            );
        }
        _ => {
            msg.push_str(
                "  Download Saxon-HE from:\n  \
                 https://github.com/Saxonica/Saxon-HE/releases\n",
            );
        }
    }

    msg
}

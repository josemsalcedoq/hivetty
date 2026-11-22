//! `cargo xtask manifests <version> <packages-dir>`: fills the Homebrew cask and the winget
//! manifests in `packaging/` with a release's file names and SHA-256 sums (F6-10), into
//! `target/manifests/`. Submitting them needs the project's accounts; this only prepares them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Templates, relative to the repository root, and where each rendered file goes.
const TEMPLATES: &[(&str, &str)] = &[
    ("packaging/homebrew/hivetty.rb", "homebrew/hivetty.rb"),
    (
        "packaging/winget/Hivetty.Hivetty.yaml",
        "winget/Hivetty.Hivetty.yaml",
    ),
    (
        "packaging/winget/Hivetty.Hivetty.installer.yaml",
        "winget/Hivetty.Hivetty.installer.yaml",
    ),
    (
        "packaging/winget/Hivetty.Hivetty.locale.en-US.yaml",
        "winget/Hivetty.Hivetty.locale.en-US.yaml",
    ),
];

/// The values a template may use.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Values {
    pub(crate) version: String,
    pub(crate) dmg: String,
    pub(crate) dmg_sha256: String,
    pub(crate) msi: String,
    pub(crate) msi_sha256: String,
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// The one file in `dir` with `extension`.
fn find(dir: &Path, extension: &str) -> Result<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        })
        .collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => bail!("no .{extension} in {}", dir.display()),
        _ => bail!("more than one .{extension} in {}", dir.display()),
    }
}

/// Reads the release's `.dmg` and `.msi` from `dir`.
pub(crate) fn values(version: &str, dir: &Path) -> Result<Values> {
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))
    {
        bail!("invalid version '{version}'");
    }
    let dmg = find(dir, "dmg")?;
    let msi = find(dir, "msi")?;
    let name = |p: &Path| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    Ok(Values {
        version: version.to_owned(),
        dmg: name(&dmg),
        dmg_sha256: sha256_file(&dmg)?,
        msi: name(&msi),
        msi_sha256: sha256_file(&msi)?,
    })
}

/// Fills one template. The comment that says it is a template is dropped; a placeholder left
/// over is an error.
pub(crate) fn render(template: &str, v: &Values) -> Result<String> {
    let text: String = template
        .lines()
        .filter(|l| !(l.starts_with('#') && l.contains("A template")))
        .filter(|l| !(l.starts_with('#') && l.contains("fills {{")))
        .map(|l| format!("{l}\n"))
        .collect();
    let out = text
        .replace("{{version}}", &v.version)
        .replace("{{dmg}}", &v.dmg)
        .replace("{{dmg_sha256}}", &v.dmg_sha256)
        .replace("{{msi}}", &v.msi)
        .replace("{{msi_sha256_upper}}", &v.msi_sha256.to_ascii_uppercase())
        .replace("{{msi_sha256}}", &v.msi_sha256);
    if let Some(i) = out.find("{{") {
        bail!(
            "unknown placeholder near '{}'",
            &out[i..(i + 30).min(out.len())]
        );
    }
    Ok(out)
}

/// The command.
pub(crate) fn run(args: &[String]) -> Result<()> {
    let [version, dir] = args else {
        bail!("usage: cargo xtask manifests <version> <packages-dir>");
    };
    let values = values(version, Path::new(dir))?;
    let out = Path::new("target/manifests");
    for (template, target) in TEMPLATES {
        let text =
            std::fs::read_to_string(template).with_context(|| format!("cannot read {template}"))?;
        let path = out.join(target);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, render(&text, &values)?)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_render_completely() {
        let dir = std::env::temp_dir().join(format!("hv-manifests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Hivetty_1.2.3_universal.dmg"), b"dmg").unwrap();
        std::fs::write(dir.join("Hivetty_1.2.3_x64_en-US.msi"), b"msi").unwrap();
        let v = values("1.2.3", &dir).unwrap();
        assert_eq!(v.msi, "Hivetty_1.2.3_x64_en-US.msi");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for (template, _) in TEMPLATES {
            let text = std::fs::read_to_string(root.join(template)).unwrap();
            let out = render(&text, &v).unwrap();
            assert!(out.contains("1.2.3"), "{template}");
            assert!(!out.contains("A template"), "{template}");
        }
        let cask = render(
            &std::fs::read_to_string(root.join("packaging/homebrew/hivetty.rb")).unwrap(),
            &v,
        )
        .unwrap();
        assert!(cask.contains(&v.dmg_sha256));
        assert!(render("x {{nope}}", &v).is_err());
        assert!(values("1.2.3; rm", &dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

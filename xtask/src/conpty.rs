//! `cargo xtask conpty-fetch [--profile debug|release]`
//!
//! Downloads Microsoft's ConPTY NuGet package (MIT), verifies its SHA-256 and places
//! `conpty.dll` + `OpenConsole.exe` next to the hivetty executable. `portable-pty` loads
//! `conpty.dll` from the executable's directory when present, and `conpty.dll` runs the
//! bundled `OpenConsole.exe` instead of the system `conhost.exe` (docs/platforms.md).

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Pinned package. Update both values together (and re-run SP-2).
const VERSION: &str = "1.25.261109001";
const SHA256: &str = "3d77f535df848e44225c2a8912ac844fecb23f3057732b639f3034b587496f09";

pub(crate) fn fetch(args: &[String]) -> Result<()> {
    let profile = match args.iter().position(|a| a == "--profile") {
        Some(i) => args.get(i + 1).context("--profile needs a value")?.as_str(),
        None => "debug",
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        other => bail!("ConPTY is not published for {other}"),
    };

    let root = workspace_root()?;
    let cache = root.join("target").join("conpty-cache");
    fs::create_dir_all(&cache)?;
    let package = cache.join(format!("conpty-{VERSION}.nupkg"));

    let bytes = match fs::read(&package) {
        Ok(bytes) if sha256_hex(&bytes) == SHA256 => bytes,
        _ => {
            let url = format!(
                "https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/{VERSION}"
            );
            println!("downloading {url}");
            let mut bytes = Vec::new();
            ureq::get(&url)
                .call()
                .context("download failed")?
                .into_body()
                .into_reader()
                .read_to_end(&mut bytes)?;
            let got = sha256_hex(&bytes);
            if got != SHA256 {
                bail!("checksum mismatch for ConPTY {VERSION}: expected {SHA256}, got {got}");
            }
            fs::write(&package, &bytes)?;
            bytes
        }
    };

    let out_dir = root.join("target").join(profile);
    fs::create_dir_all(&out_dir)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("not a valid nupkg")?;
    for (inner, name) in [
        (
            format!("runtimes/win-{arch}/native/conpty.dll"),
            "conpty.dll",
        ),
        (
            format!("build/native/runtimes/{arch}/OpenConsole.exe"),
            "OpenConsole.exe",
        ),
    ] {
        let mut entry = zip
            .by_name(&inner)
            .with_context(|| format!("{inner} missing in package"))?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        let dest = out_dir.join(name);
        fs::write(&dest, data).with_context(|| format!("cannot write {}", dest.display()))?;
        println!("installed {}", dest.display());
    }
    println!("ConPTY {VERSION} ({arch}) ready in {}", out_dir.display());
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .context("xtask must live inside the workspace")
}

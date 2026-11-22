# SP-7 — Paquetes sin firmar en CI: ¿qué herramienta de empaquetado?

- **Fecha:** 2026-11-21
- **Estado:** ✅ superado (paquetes sin firmar; la firma y la notarización quedan para F6)
- **Rama:** `spike/sp-7-packaging`. Workflow: `.github/workflows/package.yml`

## Qué se probó
- **cargo-packager 0.11.8** (CrabNebula) es la única candidata que cubre los cuatro formatos (dmg, deb, AppImage y MSI) con una sola configuración (`[package.metadata.packager]` en `crates/hv-app/Cargo.toml`). Se probó primero y funcionó a la primera, así que no hizo falta probar las demás:
  - **dist (cargo-dist)** no genera dmg, deb ni AppImage (solo MSI e instaladores por script).
  - **cargo-bundle + cargo-deb + cargo-wix + appimagetool** son cuatro herramientas con cuatro configuraciones distintas, y cargo-bundle sigue en "very early alpha".
- Workflow `package.yml`: se lanza con `workflow_dispatch` y con etiquetas `v*` (no en cada push). Usa una matriz macOS (`macos-latest`, arm64), `ubuntu-24.04` y `windows-latest`. En cada SO, `cargo build --release -p hv-app` y después `cargo packager --release --formats <por SO>`. Las Actions están fijadas por SHA, igual que en `ci.yml`.
- cargo-packager no publica binarios ni está en `taiki-e/install-action`, así que se instala con `cargo install cargo-packager --version 0.11.8 --locked` (de 70 a 115 s por job).
- **ConPTY en Windows:** `cargo xtask conpty-fetch --profile release` deja `conpty.dll` y `OpenConsole.exe` en `target/release`. Se declaran como `resources` (`../../target/release/conpty.dll` y `OpenConsole.exe`). En WiX, los recursos se instalan junto al exe. En macOS y Linux esos globs no encuentran nada y se ignoran, así que basta con una sola configuración.
- Icono provisional (`crates/hv-app/packaging/icon.png` de 512 px e `icon.ico` multitamaño), generado con un script. cargo-packager crea el `.icns` a partir del PNG.
- Identificador `dev.hivetty.Hivetty`, nombre de producto `Hivetty` y paquete deb `hivetty`.

## Resultados
Primera ejecución (por push, con la caché vacía): [44969838127](https://github.com/josemsalcedoq/hivetty/actions/runs/44969838127). Segunda ejecución (`gh workflow run package.yml --ref spike/sp-7-packaging`, con la caché llena): [44970680586](https://github.com/josemsalcedoq/hivetty/actions/runs/44970680586). Las dos salieron en verde en los tres SO.

| SO | Artefacto | Tamaño | Job (caché vacía / llena) |
|---|---|---|---|
| macOS (arm64) | `Hivetty_0.0.0_aarch64.dmg` (contiene `Hivetty.app`) | 5.5 MiB | 4 min 26 s / 2 min 0 s |
| Linux | `hivetty_0.0.0_amd64.deb` | 7.8 MiB | 4 min 41 s / 1 min 46 s |
| Linux | `hivetty_0.0.0_x86_64.AppImage` | 8.1 MiB | (el mismo job) |
| Windows | `hivetty_0.0.0_x64_en-US.msi` | 7.8 MiB | 6 min 51 s / 2 min 50 s |

Los artefactos se suben como `hivetty-<SO>-unsigned`. El paso de empaquetado tarda de 4 a 23 s; con la caché vacía, casi todo el tiempo se va en compilar en release y en instalar cargo-packager.

Nota: `workflow_dispatch` funcionó en la rama sin que el workflow estuviera aún en `main`, después de que un primer push lo registrara (ese push llevaba un disparador temporal por rama, ya eliminado).

Comprobaciones de contenido, hechas leyendo los paquetes sin instalarlos:
- **MSI:** `INSTALLDIR` = `Program Files\Hivetty`, con `hivetty.exe`, `conpty.dll` y `OpenConsole.exe` en la misma carpeta (lo que pide `docs/platforms.md`). Crea accesos directos en el menú Inicio y en el escritorio, y otro de desinstalación.
- **deb:** `usr/bin/hivetty`, `usr/share/applications/hivetty.desktop` (categoría `Development`) y el icono en `hicolor/512x512`. Sección `x11`.
- **dmg:** `Hivetty.app` para arm64. La maquetación con AppleScript y Finder funcionó en el runner sin problemas.

## Decisión
**cargo-packager** para dmg, deb, AppImage y MSI, como ya proponía `docs/release.md` §2. Una sola herramienta y una sola configuración, y la generación de los cuatro formatos se integra en CI en pocos minutos. Las actualizaciones automáticas siguen previstas con Velopack (F6-11), fuera del alcance de este spike. Plan B si cargo-packager se estanca (la última versión es de noviembre de 2025): scripts propios en `xtask` (`hdiutil`, `dpkg-deb`, `appimagetool` y WiX).

## Pendiente
- **macOS (F6-07):** binario universal (`lipo` de arm64 y x86_64; ahora solo hay arm64), firma con Developer ID y hardened runtime, notarización con `notarytool` y grapado. Fondo y maquetación del dmg.
- **Linux (F6-08):** declarar `Depends` en el deb (fontconfig, xkbcommon, Vulkan/Mesa…; ahora va vacío), registrar `x-terminal-emulator` con `update-alternatives`, `.desktop` propio, revisar qué bibliotecas mete linuxdeploy en el AppImage y probar ambos en Ubuntu limpio, Fedora y Arch. Builds arm64. Firma GPG.
- **Windows (F6-09):** firmar el exe, `conpty.dll`, `OpenConsole.exe` y el MSI (Azure Artifact Signing u OV/EV). Poner el AUMID en el acceso directo del menú Inicio (plantilla WiX propia o fragmento). Decidir si quitar el acceso directo del escritorio. Build arm64. MSIX si se descarta el MSI. cargo-packager usa WiX 3.11, que ya no tiene soporte; vigilar si migra a WiX 4+.
- **Canales (F6-10):** manifiesto de winget, Homebrew cask, Scoop y repo APT.
- **Versiones:** el workspace está en `0.0.0`. El MSI exige versiones numéricas (`major ≤ 255`, `minor ≤ 255`, `patch ≤ 65535`), lo que afecta a las etiquetas `-beta.N`.
- **Release:** `release.yml` con el entorno `release` y los secretos de firma, `SHA256SUMS` y minisign, SBOM y atestaciones (F6-15). Nombres de los artefactos alineados con `docs/release.md` §5 (ahora: `Hivetty_<v>_aarch64.dmg` y `hivetty_<v>_x64_en-US.msi`).
- Con la caché vacía, instalar cargo-packager cuesta entre 1 y 2 min por job. Para las releases, valorar una caché dedicada o un binario fijado por hash.
- Icono definitivo.

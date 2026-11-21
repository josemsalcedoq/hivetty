# CI, empaquetado, firma y distribución

Herramientas verificadas el 2026-11-21.

## 1. CI (`.github/workflows/ci.yml`, se crea en F0-04)

```yaml
name: ci
on: [push, pull_request]
permissions: { contents: read }
jobs:
  check:
    strategy:
      fail-fast: false
      matrix: { os: [macos-latest, ubuntu-24.04, windows-latest] }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@<sha>
      - uses: dtolnay/rust-toolchain@<sha>   # lee rust-toolchain.toml
      - uses: Swatinem/rust-cache@<sha>
      - if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev libvulkan-dev mesa-vulkan-drivers zsh fish
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets --locked -- -D warnings
      - uses: taiki-e/install-action@<sha>
        with: { tool: cargo-nextest,cargo-deny }
      - run: cargo nextest run --workspace --locked
      - run: cargo deny check
```

Workflows adicionales:
- `nightly.yml`: fuzzing (30 min por objetivo), `cargo audit`, `esctest2` y bancos de rendimiento con comparación.
- `release.yml`: se dispara con una etiqueta `v*` y corre en el entorno `release` (con aprobación manual y los secretos de firma).
- `zizmor` para auditar los propios workflows.

GPU en CI: lavapipe (`mesa-vulkan-drivers`) en Linux, WARP en Windows y Metal en macOS (`docs/testing.md` §3).

## 2. Herramientas de empaquetado

| Herramienta | Formatos | Estado | Uso en hivetty |
|---|---|---|---|
| **cargo-packager** (CrabNebula) | `.app`, **dmg**, **deb**, **AppImage**, pacman, **NSIS**, **MSI** + `cargo-packager-updater` | 0.11.8 (nov 2025); el ritmo ha bajado | **candidata principal** para los artefactos (SP-7) |
| dist (cargo-dist) | shell, powershell, npm, homebrew, **msi** (dmg y app aún no; deb y AppImage no) | 0.34.0 (nov 2026), activo | útil para los instaladores por script y Homebrew; no cubre dmg, deb ni AppImage |
| cargo-bundle | `.app`, dmg, deb, AppImage, rpm y msi (experimental) | 0.12.0, "very early alpha" | descartada |
| **Velopack** (crate `velopack` 1.2) | Windows: Setup.exe + deltas; Linux: AppImage auto-actualizable; macOS: `.pkg` | activo; el CLI `vpk` es una herramienta .NET | **candidata para las actualizaciones automáticas** (deltas, firma integrada) |

Decisión en SP-7: **cargo-packager** para dmg, deb, AppImage y MSI, y **Velopack** para las actualizaciones (Windows y Linux). En macOS, actualizaciones con Velopack (`.pkg`) o con un feed propio firmado. Si cargo-packager se estanca, plan B: scripts propios en `xtask` (`hdiutil`, `dpkg-deb`, `appimagetool`, WiX 5).

## 3. Firma

### macOS
1. Cuenta de Apple Developer (99 $/año) con un certificado **Developer ID Application**.
2. En CI: importar el `.p12` en un llavero temporal.
3. `codesign --options runtime --timestamp --entitlements hivetty.entitlements` (hardened runtime) sobre el binario universal (`lipo` de arm64 y x86_64).
4. `xcrun notarytool submit hivetty.dmg --key … --key-id … --issuer … --wait` (con clave de API de App Store Connect; `altool` ya no sirve).
5. `xcrun stapler staple hivetty.dmg`.
6. Sin sandbox (un terminal no puede ir en sandbox), así que no hay Mac App Store.

### Windows
- **Azure Artifact Signing** (antes "Trusted Signing", GA desde enero de 2026). Es la opción más barata para tener reputación de SmartScreen.
  - ⚠️ **Como particular solo está disponible en EE. UU. y Canadá.** Como organización: EE. UU., Canadá, UE, Reino Unido, Australia, Nueva Zelanda, Japón, Corea, Singapur, Suiza, Noruega e Israel. Validación de 1 a 30 días laborables.
  - Si no hay organización en esos países: certificado OV o EV de una CA (Sectigo, DigiCert, SSL.com…), normalmente en un token o HSM en la nube.
- En CI: `azure/artifact-signing-action@v2` (solo en runners de Windows), o `vpk --azureTrustedSignFile` si se usa Velopack 1.3.
- Firmar el exe, `conpty.dll`, `OpenConsole.exe` y el instalador.

### Linux
- Firmar con GPG el repositorio APT y los AppImage (`--sign`).
- Opcional: Flathub, pero un terminal dentro de Flatpak necesita `flatpak-spawn --host` para lanzar shells. Documentar o descartar. Snap necesitaría confinamiento *classic*.

### Feed de actualizaciones
Firma ed25519 propia (minisign) además de la del SO, y comprobación de que la versión no baja (`docs/security.md` §2.7).

## 4. Notificaciones en Windows (requisito del instalador)
Una app de escritorio sin empaquetar **no puede mostrar toasts sin un AUMID** registrado mediante un acceso directo en el menú Inicio. Si no, `notify-rust` (tauri-winrt-notification) usa el AUMID de PowerShell y las notificaciones aparecen como "PowerShell". El instalador debe:
1. Crear el acceso directo en el menú Inicio con la propiedad `System.AppUserModel.ID = "dev.hivetty.app"`.
2. La app llama a `SetCurrentProcessExplicitAppUserModelID` al arrancar.
3. Opcional: registrar `HKCU\Software\Classes\AppUserModelId\<AUMID>` (DisplayName, IconUri) y un activador COM para que el clic en la notificación abra la sesión.

En la versión portable (zip), avisar de que las notificaciones necesitan el instalador.

## 5. Artefactos por release

| SO | Artefactos |
|---|---|
| macOS | `hivetty-<v>-universal.dmg` (firmado, notarizado y grapado) |
| Windows | `hivetty-<v>-x64.msi`, `hivetty-<v>-arm64.msi`, Setup de Velopack y un zip portable |
| Linux | `hivetty_<v>_amd64.deb`, `hivetty_<v>_arm64.deb`, `hivetty-<v>-x86_64.AppImage`, `hivetty-<v>-aarch64.AppImage` |
| Todos | `SHA256SUMS` + firma minisign, SBOM CycloneDX, `THIRD_PARTY_NOTICES` y atestación de procedencia |

## 6. Canales
- **macOS:** Homebrew cask (tap propio y después homebrew-cask oficial).
- **Windows:** winget (PR a `microsoft/winget-pkgs` automatizado con `wingetcreate`) y Scoop (bucket propio).
- **Linux:** repo APT propio (GitHub Pages + GPG), AppImage en GitHub Releases y AUR (lo mantiene la comunidad). En el paquete `.deb`, registrar el binario con `update-alternatives` como `x-terminal-emulator`.

## 7. Versionado y notas
- Etiquetas `vX.Y.Z`. El changelog se genera con `git-cliff` desde Conventional Commits.
- Canal `beta` (etiquetas `vX.Y.Z-beta.N`) para probar antes de estable.
- Notas de versión con capturas y la lista de agentes y versiones probados.

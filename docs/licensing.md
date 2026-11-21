# Licencias, marcas y nombre

## Licencia del proyecto
- Propuesta: **MIT OR Apache-2.0** (doble, lo habitual en Rust).
- Decidirlo antes del primer commit de código (F0). Si se quiere evitar que otros lo cierren, usar GPL-3.0 (ojo: no podría reutilizarse en productos cerrados).

## Dependencias
- `cargo-deny` con lista de licencias permitidas: MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, 0BSD, BSL-1.0, CC0-1.0, MPL-2.0 (solo como dependencia), Unicode-3.0 y OFL-1.1 (fuentes).
- Prohibidas por defecto: GPL/AGPL (salvo si el proyecto se licencia como GPL), y cualquier licencia sin identificar.
- Avisos de terceros (`THIRD_PARTY_NOTICES`) generados con `cargo-about` e incluidos en cada paquete.

## Fuentes incluidas
- JetBrains Mono (OFL-1.1), Symbols Nerd Font (MIT/OFL) e Inter (OFL-1.1). Incluir sus licencias.

## Logos de agentes y tecnologías
- Son **marcas registradas**. Usarlos solo para identificar el producto (uso nominativo), sin modificarlos y siguiendo la guía de marca de cada empresa cuando la haya.
- Fuente práctica: [Simple Icons](https://simpleicons.org) (CC0 para el SVG, pero **la marca sigue siendo de su dueño**). Comprobar que cada logo está ahí o en el kit de prensa oficial.
- Alternativa segura: iconos genéricos propios, con el logo real como opción descargable.
- No usar los logos en el icono de la app, en publicidad ni de forma que sugiera respaldo oficial.

## Nombre
- Nombre elegido: **Hivetty** (2026-11-21; libre en crates.io y con solo 1 repo homónimo en GitHub). Antes de publicar, comprobar también que no choca con marcas registradas (USPTO, EUIPO, OEPM), con crates de crates.io, con paquetes de Homebrew/winget/apt ni con dominios.
- Diseño propio: no copiar el nombre, el aspecto exacto ni los recursos de productos existentes (por ejemplo, el terminal de las capturas).

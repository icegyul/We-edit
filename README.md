<p align="center">
  <img src="assets/branding/openpremier.svg" width="200" alt="Logo de OpenPremier">
</p>

<h1 align="center">OpenPremier</h1>

<p align="center">
  Editor de vídeo no lineal, libre y de código abierto, desarrollado principalmente en Rust.
</p>

<p align="center">
  <a href="https://github.com/xXKuroiKenshiXx/OpenPremier/actions/workflows/ci_build.yml"><img src="https://github.com/xXKuroiKenshiXx/OpenPremier/actions/workflows/ci_build.yml/badge.svg" alt="Estado de compilación"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licencia-GPL--3.0--or--later-blue.svg" alt="Licencia GPL-3.0-or-later"></a>
  <a href="https://github.com/xXKuroiKenshiXx/OpenPremier/releases"><img src="https://img.shields.io/github/v/release/xXKuroiKenshiXx/OpenPremier?include_prereleases&label=versi%C3%B3n" alt="Última versión"></a>
</p>

OpenPremier busca ofrecer un entorno de edición profesional con una organización y un flujo de
trabajo familiares para quienes vienen de otros editores no lineales. El proyecto es independiente,
no contiene código ni recursos de Adobe y no está afiliado con Adobe Inc.

<p align="center">
  <img src="assets/branding/Windows.png" width="70"  alt="Logowindows"> <img src="assets/branding/Linux.png" width="70" alt="Logolinux"> <img src="assets/branding/macOS.png" width="70" alt="Logomac">
</p>
## Estado actual

La versión `0.4.0` es una versión alfa funcional para Windows y Linux. La aplicación ya se puede
compilar, abrir, probar y empaquetar, pero todavía no está lista para sustituir un editor comercial
en todos los trabajos de producción.

### Funciones disponibles

- Timeline multipista con insert, overwrite, razor, ripple, roll, slip, slide, rate stretch,
  snapping, nesting, marcadores, transiciones y keyframes.
- Paneles acoplables para Proyecto, Source, Program, Timeline, Effect Controls, Effects, History,
  Audio Mixer, Meters, Markers, Info, Lumetri, Scopes, Graphics y Tools.
- Lectura, decodificación y exportación multimedia mediante FFmpeg, con exportación que se puede
  pausar, vista previa de los fotogramas renderizados y velocidad de fotogramas configurable.
- Composición por GPU mediante `wgpu`, con Vulkan, Direct3D 12 y Metal según la plataforma.
- Efectos de estilo (resplandor, fallo digital, temblor de cámara, grano, VHS, película antigua,
  fugas de luz...), transiciones de zoom, giro, destello y luz, y ajustes preestablecidos de
  animación listos para arrastrar.
- Mezclador de audio en punto flotante, medidores, efectos básicos y salida a dispositivos.
- Formato nativo `.opproj`, intercambio OTIO/FCP XML/EDL e importación parcial de `.prproj`; los
  efectos y transiciones que no existen tal cual se sustituyen por el equivalente más cercano.
- Interfaz en español e inglés e importación de mapas de teclado `.kys`.
- Pegado de imágenes con Ctrl+V desde el navegador, otros programas o el explorador de archivos:
  la imagen se guarda, se importa y queda lista en la línea de tiempo.
- Recuperación automática del proyecto ante fallos y registro de actividad configurable desde
  Preferencias.
- Paquete ZIP portátil para Windows y AppImage para Linux.

### Trabajo pendiente

- La importación `.prproj` es parcial y de sólo lectura.
- Aún faltan OpenFX, scripting Wasm, VST3 y parte del catálogo de efectos.
- Faltan pruebas más amplias de color, audio envolvente, múltiples monitores y distintos modelos de
  GPU.
- Todavía se deben completar las pruebas de compatibilidad y rendimiento para proyectos grandes.

El detalle técnico actualizado se encuentra en
[docs/implementation-status.md](docs/implementation-status.md).

## Descargar

Los instaladores y paquetes publicados estarán disponibles en
[GitHub Releases](https://github.com/xXKuroiKenshiXx/OpenPremier/releases).

Cada versión incluye:

- `OpenPremier-<versión>-windows-x64.zip`
- `OpenPremier-<versión>-x86_64.AppImage`
- `SHA256SUMS.txt`

## Compilar desde el código fuente

Se necesita Git y Rust `1.98.1` o posterior. El comando `xtask` descarga y configura las
dependencias nativas necesarias:

```text
cargo xtask build --release
cargo xtask test
cargo xtask lint
```

Para generar los paquetes de distribución:

```text
cargo xtask dist
```

En Windows, la creación del AppImage utiliza la distribución WSL `OpenPremier-Build` y Podman.
Los resultados se guardan en `dist/`.

## Comprobar un paquete

```text
OpenPremier.exe --self-test
APPIMAGE_EXTRACT_AND_RUN=1 ./OpenPremier-0.4.0-x86_64.AppImage --self-test
```

La prueba comprueba las bibliotecas de FFmpeg, los codificadores requeridos, la creación del
adaptador gráfico y una operación básica del compositor.

## Estructura del proyecto

| Ruta | Contenido |
|---|---|
| `crates/op-core` | Modelo de proyecto, tiempo exacto, parámetros, validación e historial |
| `crates/op-timeline` | Operaciones y navegación de la Timeline |
| `crates/op-project` | Formatos de proyecto e intercambio |
| `crates/op-media` | Lectura, decodificación, caché y codificación con FFmpeg |
| `crates/op-render` | Compositor GPU, efectos, transiciones, gráficos y scopes |
| `crates/op-audio` | Mezclador, DSP, medidores y dispositivos de audio |
| `crates/op-application` | Comandos, reproducción, autosave, preferencias y exportación |
| `crates/op-ui` | Interfaz, paneles, monitores, Timeline, diálogos e idiomas |
| `crates/openpremier` | Ejecutable, modo portátil, diagnóstico y self-test |
| `xtask` | Automatización de compilación, pruebas y paquetes |
| `docs` | Arquitectura, formatos, compatibilidad y estado técnico |

## Contribuir

Antes de enviar cambios, revisa [CONTRIBUTING.md](CONTRIBUTING.md) y ejecuta:

```text
cargo xtask lint
cargo xtask test
```

Los problemas de seguridad deben comunicarse siguiendo [SECURITY.md](SECURITY.md), no mediante un
issue público.

## Licencia

OpenPremier se distribuye bajo la licencia
[GNU General Public License 3.0 o posterior](LICENSE).

## ⚠️ Aviso Legal / Disclaimer

**OpenPremier** es un proyecto de código abierto desarrollado de forma 100% independiente. Este software **no tiene ninguna relación, afiliación, patrocinio ni respaldo por parte de Adobe Systems Incorporated**. 

No utilizamos, distribuimos ni tenemos acceso a ningún código fuente de Adobe Premiere Pro ni de ningún otro producto de Adobe. OpenPremier ha sido construido desde cero por la comunidad y para la comunidad, sirviendo únicamente como una inspiración basada en los estándares de la industria de la edición de video para ofrecer una alternativa libre y accesible. 

*Los nombres "Adobe" y "Premiere Pro" son marcas registradas de sus respectivos propietarios y se mencionan en este proyecto de manera puramente descriptiva y de referencia.*

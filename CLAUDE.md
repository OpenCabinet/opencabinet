<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# CLAUDE.md — Instrucciones para Claude Code

Este archivo le da contexto a Claude Code en cada sesión. El plan completo
(en español) está en [`docs/es/plan.md`](docs/es/plan.md).

## Qué es este proyecto

Suite open source, gratuita y modular para diseño, documentación y
fabricación de millwork (carpintería arquitectónica, cabinetry). Empieza por
un diseñador de gabinetes con despiece y lista de materiales; el CAD 2D propio
llega en fases posteriores.

El fundador (Andrés) es nuevo en open source. **Explica cada decisión en
español sencillo y con el porqué** antes de aplicarla.

## Fase actual: Fase 1b — Diseño, con la Fase 2 iniciada en paralelo

La Fase 1 del plan se divide en dos partes:

- **1a — Entorno (cerrada):** repositorio, licencias, archivos de comunidad,
  CI de documentación y ADRs de las decisiones ya tomadas.
- **1b — Diseño (actual):** RFC-0001 (modelo de proyecto), ADR-0009
  (longitudes), ADR-0011 (stack: Rust, aceptado), RFC-0002 (módulos y
  plugins) y ADR-0012 (interfaz: Slint, aceptado). Los RFCs pasan al menos 14 días
  en comentarios públicos antes de aceptarse.

El 2026-10-25 Andrés decidió **empezar la Fase 2 sin esperar** a que cierren
los comentarios de los RFCs (8 de noviembre). Reglas mientras tanto:

- Los RFCs se aceptan igual al cumplir sus 14 días; no se acorta un período
  anunciado.
- El código de la aplicación vive en `crates/` y empieza por lo que no
  depende de los puntos abiertos de los RFCs (workspace, CI, longitudes,
  esqueleto de la interfaz, instalador). Lo que dependa de ellos se ajusta
  cuando se acepten.
- Los **prototipos desechables** siguen en `prototypes/` y se borran cuando
  su ADR o RFC se acepta.

Alcance acordado para la versión 0.1 (Fase 2): construcción frameless **y**
face frame, operaciones de mecanizado modeladas (taller manual y CNC), y
gabinetes ubicados en paredes con elevaciones.

Todos los cambios entran por rama y pull request, nunca directo a `main`.

La Fase 0 está cerrada: el contrato laboral fue revisado por un abogado y se
autorizó publicar.

Antes de cada paso relevante, confirma con Andrés. Trabaja en pasos pequeños
y haz un commit firmado (`git commit -s`) por paso.

## Principio de tamaño

"La estructura crece cuando crece la comunidad, no antes." No crear
documentos de proceso (MAINTAINERS, CODEOWNERS, CHANGELOG con entradas, repo
`.github` de la organización) hasta que exista algo que los necesite.

## Decisiones ya tomadas (no cambiarlas sin preguntar)

- Licencia del núcleo y módulos de escritorio: **GPL-3.0-or-later**
  ("or later" confirmado en el ADR-0013).
- Servicios de servidor (futuros): **AGPL-3.0-or-later**.
- Documentación e íconos: **CC BY-SA 4.0**. Especificación del formato de
  archivo: **CC BY 4.0**. Plantillas de plugin: **MIT**.
- Archivos de configuración triviales (`.gitignore`, `.editorconfig`, etc.):
  **CC0-1.0**.
- Cumplimiento de licencias con la especificación **REUSE**
  (<https://reuse.software>), verificado en la CI con `reuse lint`. Solo se
  añade a `LICENSES/` una licencia cuando algún archivo la usa.
- Nombre del proyecto: **OpenCabinet**. Los módulos llevan nombres
  descriptivos con el prefijo de la suite: "OpenCabinet Designer",
  "OpenCabinet Shop Drawings" (ver ADR-0010).
- Nombre y logo: reservados por política de marca (TRADEMARKS.md).
- Contribuciones: **DCO** (`git commit -s`), sin CLA.
- Estructura: **monorepo**. Las plantillas de GitHub viven en el `.github/`
  del propio monorepo.
- Ramas: trunk-based; `main` protegida; todo entra por pull request; squash
  merge conservando las líneas `Signed-off-by`.
- Commits: **Conventional Commits**. Versionado: **SemVer**. CHANGELOG en
  formato Keep a Changelog (desde la primera versión).
- Idioma del repositorio: **inglés** para código, issues y documentación
  principal; documentos internos del fundador en `docs/es/`.
- ADRs en formato **MADR** dentro de `docs/adr/`.
- Unidades: **sistema imperial por defecto, métrico como opción** para el
  usuario. El modelo interno de precisión se decide en un ADR de la Fase 1b.
- Núcleo en **Rust** (ADR-0011): `cargo fmt`, `cargo clippy -D warnings` y
  pruebas en la CI. Las longitudes son enteros de 64 bits en nanómetros.
- Plugins de terceros: **aislados en un sandbox WebAssembly** (Wasmtime),
  con permisos declarados y perfil determinista para los que participan en
  el cálculo (RFC-0002). Los módulos oficiales son crates de Rust.
- Interfaz gráfica de escritorio en **Slint** (ADR-0012), con egui como
  alternativa. Slint es GPL-3.0-only: el programa distribuido queda en la
  práctica bajo GPL-3.0.
- Equipo modesto (ADR-0014): equipo de referencia de unos diez años (2
  núcleos, 4 GB, sin GPU, 1366 × 768) y presupuestos de rendimiento que la
  CI vigila desde la Fase 2.
- Exportación (ADR-0015): PDF con **krilla**, DXF con el crate **`dxf`**
  (DXF 2000+ en milímetros, auditado con ezdxf en la CI) y la fuente
  **Noto Sans** (OFL-1.1) incluida como archivo aparte, no dentro del
  ejecutable.

## Decisiones pendientes (no tomarlas por tu cuenta)

- Representación interna de longitudes y precisión (ADR-0009, propuesto:
  nanómetros enteros).

## Reglas de propiedad intelectual (obligatorias)

- Nunca copiar código, íconos, textos, plantillas, bibliotecas ni
  documentación de AutoCAD, Bluebeam, Microvellum, Cabinet Vision, Mozaik u
  otro software propietario.
- No usar marcas de terceros en nombres, logos o identificadores del
  proyecto.
- Los archivos de prueba se crean desde cero para el proyecto.
- Cada archivo lleva encabezado SPDX (`SPDX-FileCopyrightText` y
  `SPDX-License-Identifier`); los que no admiten comentarios se declaran en
  `REUSE.toml`.
- Textos de licencias (GPL, CC, MIT, Contributor Covenant) deben ser los
  oficiales completos, sin modificar, descargados de la fuente oficial.

## Cuentas de GitHub

- Repositorio: <https://github.com/OpenCabinet/opencabinet>, en la
  organización `OpenCabinet` (antes `A-newGP/millwork-suite`; GitHub
  redirige los enlaces viejos). Usa `-R OpenCabinet/opencabinet` con `gh`.
- El proyecto usa **solo la cuenta personal `A-newGP`**. Nunca usar la
  cuenta laboral `agonzales-metric` ni la organización de su empleador para
  nada de este proyecto (ni push, ni issues, ni forks).
- Antes de cualquier operación con `gh`, verificar con `gh auth status` qué
  cuenta está activa.

## Lo que Claude Code NO debe hacer

- Crear la organización de GitHub o cambiar ajustes de seguridad del repo.
  Esos pasos los hace Andrés en la web; tú le das la guía paso a paso
  ([`docs/es/guia-github.md`](docs/es/guia-github.md)).
- Hacer push sin permiso explícito de Andrés en esa sesión.
- Manejar contraseñas, tokens ni secretos.
- Escribir código de la aplicación antes de la Fase 2 (los prototipos
  desechables de `prototypes/` sí están permitidos).

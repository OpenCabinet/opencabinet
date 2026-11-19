<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Guía de configuración de GitHub (para Andrés)

Estos ajustes los haces tú en la web de GitHub: tocan la seguridad de la
cuenta y del repositorio, y deben quedar bajo tu control. Para cada paso se
explica el porqué. Hazlos en orden; los pasos 1 a 6 son obligatorios antes de
invitar a nadie.

## 0. Usa siempre tu cuenta personal

En este equipo `gh` tiene dos cuentas, y la activa suele ser la laboral
(`agonzales-metric`). **Nada de este proyecto debe pasar por la cuenta ni la
organización de tu empleador**: mezclaría la propiedad intelectual que el
abogado ya revisó.

```bash
gh auth switch -u A-newGP
```

```bash
gh auth status
```

Comprueba que `A-newGP` aparezca como `Active account: true` antes de
cualquier operación.

## 1. Asegura tu cuenta

1. **Settings → Password and authentication → Two-factor authentication**:
   actívala con una app TOTP o, mejor, una llave de seguridad o passkey.
   *Por qué:* si alguien entra a tu cuenta, puede publicar código malicioso a
   nombre del proyecto.
2. Descarga los **recovery codes** y guárdalos fuera de la computadora (gestor
   de contraseñas o papel en un lugar seguro).
3. Opcional, recomendado: **SSH and GPG keys → New SSH key** de tipo
   *Signing key*, para que tus commits aparezcan como "Verified".

## 2. Repositorio y, más adelante, organización

Mientras el nombre del proyecto no esté decidido, el repositorio vive en tu
cuenta personal. Cuando lo esté:

1. Crea la organización con el nombre definitivo (**+ → New organization**,
   plan Free).
2. Transfiere el repositorio (**Settings → General → Danger Zone → Transfer**).
   GitHub redirige automáticamente los enlaces antiguos.
3. Exige 2FA a todos sus miembros (**Organization settings → Authentication
   security**).
4. Suma un **segundo propietario** de confianza con 2FA. *Por qué:* si pierdes
   el acceso, el proyecto no queda huérfano (ver GOVERNANCE.md).

## 3. Ajustes generales del repositorio

**Settings → General:**

- **Features:** Issues ✅, Discussions ✅, Wikis ❌ (la documentación vive en
  el repo, revisada por PR), Projects ❌ por ahora.
- **Pull Requests:**
  - Allow merge commits ❌, Allow rebase merging ❌, **Allow squash merging ✅**
    con *Default commit message* = **Pull request title and commit details**.
    *Por qué:* así el commit final en `main` conserva las líneas
    `Signed-off-by` (DCO) de cada commit del PR.
  - Always suggest updating pull request branches ✅.
  - Automatically delete head branches ✅.

## 4. Protección de `main` (ruleset)

**Settings → Rules → Rulesets → New ruleset → New branch ruleset:**

- **Name:** `main`; **Enforcement status:** Active.
- **Target branches:** *Include default branch*.
- **Bypass list:** vacía. *Por qué:* las reglas también te protegen de ti
  mismo (un push forzado accidental borra historia).
- Reglas:
  - Restrict deletions ✅
  - Require linear history ✅
  - **Require a pull request before merging** ✅
    - *Required approvals:* **0 mientras seas el único mantenedor** (GitHub
      no te deja aprobar tus propios PR; con 1 te bloquearías). Súbelo a 1
      cuando llegue el segundo mantenedor.
    - Dismiss stale approvals when new commits are pushed ✅
    - Require conversation resolution before merging ✅
    - *Allowed merge methods:* solo **Squash**.
  - **Require status checks to pass** ✅ con *Require branches to be up to
    date* ✅. Agrega los checks (aparecen tras la primera ejecución de la CI):
    `Markdown`, `REUSE licensing`, `DCO sign-off`, `Conventional Commits title`.
  - Block force pushes ✅

Desde ese momento, todo cambio entra por PR, incluidos los tuyos.

## 5. Seguridad

**Settings → Advanced Security** (en algunas cuentas se llama *Code security*):

- **Private vulnerability reporting:** Enable. *Por qué:* es el canal que
  promete SECURITY.md.
- **Dependency graph** ✅, **Dependabot alerts** ✅, **Dependabot security
  updates** ✅.
- **Secret Protection / Secret scanning** ✅ y **Push protection** ✅. *Por
  qué:* bloquea un push que contenga por error un token o contraseña.

## 6. GitHub Actions

**Settings → Actions → General:**

- **Actions permissions:** *Allow owner, and select non-owner, actions and
  reusable workflows*, con:
  - Allow actions created by GitHub ✅
  - Lista permitida: `DavidAnson/markdownlint-cli2-action@*, fsfe/reuse-action@*`
  - Si aparece, **Require actions to be pinned to a full-length commit SHA** ✅
    (nuestros workflows ya lo hacen).
- **Fork pull request workflows:** *Require approval for all external
  contributors*. *Por qué:* evita que un desconocido ejecute código en tu CI
  sin que lo revises.
- **Workflow permissions:** *Read repository contents and packages
  permissions*; Allow GitHub Actions to create and approve pull requests ❌.

## 7. Etiquetas (labels)

Las plantillas de issues usan estas etiquetas. Con tu cuenta personal
activa, desde la carpeta del repo:

```bash
gh label create "type: proposal" --color 1D76DB --description "Feature or design proposal" --force
```

```bash
gh label create "type: docs" --color 0075CA --description "Documentation" --force
```

```bash
gh label create "status: triage" --color FBCA04 --description "Waiting for a maintainer to review" --force
```

```bash
gh label create "good first issue" --color 7057FF --description "Good for newcomers" --force
```

## 8. Discussions

**Discussions → Categories:** deja *Announcements*, *General*, *Ideas*,
*Q&A* y crea **Millwork know-how** (cómo trabajan los talleres: métodos
constructivos, holguras, convenciones). Esa categoría alimenta el RFC-0001.

## Estado

Hecho el 2026-10-25: pasos 1 a 8, la organización
[OpenCabinet](https://github.com/OpenCabinet) y la transferencia del
repositorio a <https://github.com/OpenCabinet/opencabinet>. Las etiquetas del
paso 7 las creó Claude Code con tu cuenta personal.

## Pendientes

- Sumar un segundo propietario a la organización (paso 2).
- Registrar `opencabinet.org` (estaba libre el 2026-10-25; el `.com` está
  ocupado).
- Consultar al abogado la búsqueda de marca (USPTO, SENAPI) y mencionarle el
  proyecto pequeño `xiamingyang70/opencabinet`, creado en octubre de 2026,
  del mismo rubro.
- Dominio propio y correos por función (`security@`, `conduct@`,
  `contact@`), con renovación automática y dos personas con acceso.
- Reemplazar el correo de reportes en CODE_OF_CONDUCT.md (hoy tu Gmail) por `conduct@`.

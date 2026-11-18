<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Millwork Suite Open Source — Propuesta revisada

Versión: 11 de octubre de 2026 · Autor: Andrés

## Resumen ejecutivo

La suite empezará por un módulo de millwork usable, no por un núcleo CAD propio. Se licenciará como GPLv3-o-posterior en escritorio y AGPLv3-o-posterior en servidores.

Cambios clave respecto a la propuesta inicial:

- **Orden de construcción.** Primero un diseñador de gabinetes con despiece y lista de materiales. El CAD 2D propio llega cuando exista comunidad. Lo que diferencia al proyecto es el conocimiento de millwork, no otro CAD genérico.
- **El núcleo real es el modelo de datos.** Un formato de proyecto abierto y documentado que todos los módulos comparten. La geometría es un servicio más.
- **Reutilizar antes que reescribir.** Geometría, DXF/DWG y PDF se apoyan en librerías libres existentes compatibles con GPLv3.
- **La gratuidad se protege con tres herramientas distintas:** la licencia (código abierto), la política del registro (catálogo oficial gratuito) y la marca (quién puede llamarse oficial). La licencia no puede prohibir el comercio; las otras dos sí pueden ordenar el catálogo propio.
- **Gobernanza honesta.** Fundador-mantenedor con reglas públicas y un plan escrito de transición hacia un consejo técnico.
- **Riesgo legal número uno:** el contrato laboral del fundador. Se revisa con un abogado antes de publicar nada.

## Visión y principios

Misión: herramientas profesionales, libres y modulares para diseño, documentación y fabricación de millwork, sostenidas por una comunidad.

Principios que guían cada decisión:

1. **Libre para siempre.** El código derivado que se distribuya debe seguir abierto (copyleft).
2. **Útil pronto.** Cada fase entrega algo que un taller pueda usar en trabajo real.
3. **Los datos son del usuario.** Formatos de archivo abiertos y documentados; nadie queda atrapado.
4. **Funciona sin Internet y sin IA.** Las funciones esenciales nunca dependen de un servicio externo ni de pago.
5. **Modular de verdad.** Se instala solo lo que se usa; el núcleo corre en equipos modestos.
6. **Seguro por diseño.** Un plugin no accede a archivos, red ni datos sin permisos declarados.
7. **Honesto sobre su tamaño.** La estructura crece cuando crece la comunidad, no antes.

Qué significa "gratuita" (definición propuesta): la distribución oficial y los módulos y plugins del catálogo oficial no cobran por licencia, descarga ni acceso a funciones. Servicios opcionales (soporte, formación, implementación, alojamiento) sí pueden cobrarse, por el proyecto o por terceros. Esta línea debe quedar escrita antes de aceptar contribuciones externas.

## A. Licenciamiento

Recomendación: GPL-3.0-or-later para escritorio, AGPL-3.0-or-later para servicios de servidor, CC BY-SA 4.0 para documentación y la marca fuera de toda licencia libre.

### Comparación frente a los objetivos

| Licencia | Tipo | ¿Permite forks cerrados? | Versión modificada ofrecida en la nube | Plugins propietarios |
| --- | --- | --- | --- | --- |
| GPLv3 | Copyleft fuerte | No, si se distribuye el programa | No obliga a publicar el código | En el mismo proceso: probablemente deben ser GPL (interpretación de la FSF, no resuelta judicialmente). Como programa separado: posibles |
| AGPLv3 | Copyleft de red | No | Obliga a ofrecer el código a quienes la usan por red | Igual que GPLv3 |
| MPL-2.0 | Copyleft por archivo | Solo los archivos modificados quedan abiertos | No obliga | Sí, en archivos propios |
| Apache-2.0 | Permisiva | Sí | No obliga | Sí |

MPL-2.0 y Apache-2.0 permiten que partes importantes queden propietarias. Eso contradice el objetivo de que nada derivado se cierre, por eso quedan descartadas para el núcleo.

### La nube

Con GPLv3, una empresa puede modificar el programa, ofrecerlo como servicio y no publicar sus cambios: la obligación nace al distribuir copias, no al dar acceso remoto. AGPLv3 cierra esa brecha. Solo tiene sentido en componentes que realmente corran en servidor (colaboración, revisión compartida, registro de plugins).

### Plugins y librerías de terceros

- Toda dependencia de la distribución oficial debe ser compatible con GPLv3. Ejemplos a verificar: LibreDWG (GPLv3), Open CASCADE (LGPL-2.1 con excepción), MuPDF (AGPLv3, combinable con GPLv3).
- SDKs propietarios, como el de la Open Design Alliance para DWG, no pueden entrar en la distribución oficial.
- Si los plugins corren dentro del mismo proceso y comparten estructuras de datos, el proyecto puede sostener que deben ser GPL. Si corren como procesos separados, no. Esta es una decisión de arquitectura con efecto legal (ver C).

### Estrategia por componente

| Componente | Licencia | Por qué |
| --- | --- | --- |
| Núcleo y módulos oficiales de escritorio | GPL-3.0-or-later | Protege los derivados distribuidos |
| Servicios de servidor | AGPL-3.0-or-later | Cubre el uso por red |
| Especificación del formato de archivo | CC BY 4.0, publicada aparte | Cualquiera, incluso software cerrado, puede leer los archivos del usuario: favorece la interoperabilidad |
| Plantillas y ejemplos de plugin | MIT | Copiar la plantilla no impone nada; el plugin igual queda sujeto a la GPL si se combina con el núcleo |
| Documentación e íconos de interfaz | CC BY-SA 4.0 | Libres, y los derivados siguen libres |
| Nombre, logo e insignia "oficial" | Reservados por política de marca | La licencia del código no da derecho a presentarse como oficial |

"Or later" permite adoptar futuras versiones de la GPL sin pedir permiso a cada contribuidor. A cambio, delega en la FSF la redacción de esas versiones. Es una decisión a cerrar en un ADR.

### Obligación jurídica, arquitectura o política

| Tipo | Ejemplos |
| --- | --- |
| Obligación de la licencia | Entregar el código fuente al distribuir binarios; conservar avisos; misma licencia en derivados; no añadir restricciones |
| Decisión de arquitectura | Plugins en proceso o separados; existencia de servidor; qué dependencias se integran |
| Política comunitaria | Requisitos del registro; uso de la marca; DCO; código de conducta |

Contribuciones: DCO (firma Signed-off-by en cada commit), sin CLA. Cada autor conserva su copyright y licencia su aporte bajo la licencia del proyecto.

## B. Registro oficial de plugins y marca

Exigir que el catálogo oficial sea gratuito y abierto es viable como política editorial del propio catálogo. No es viable como cláusula añadida a la licencia.

### Lo que el proyecto sí puede exigir para entrar y permanecer

- Licencia aprobada por la OSI y compatible con GPLv3.
- Código fuente público e instrucciones para compilarlo desde cero.
- Ninguna función bloqueada tras un pago dentro del catálogo; nada de telemetría oculta.
- Mantenedores identificados y un canal para reportar vulnerabilidades.
- Paquetes firmados, permisos declarados y pruebas de compatibilidad con la API vigente.
- Respuesta a reportes de seguridad en el plazo que fije la política; si no, retiro con aviso previo.

### Lo que el proyecto no puede hacer

- **Prohibir que se venda un plugin GPL fuera del registro.** La GPL garantiza el derecho a vender copias.
- **Añadir "uso no comercial" a la GPL.** La GPLv3 permite al receptor eliminar restricciones adicionales no autorizadas.
- **Impedir plugins que funcionen como programas separados**, comunicados por archivos o red. Ahí la GPL del núcleo probablemente no alcanza.
- **Impedir que alguien publique un plugin sin pasar por el registro.**

### Servicios externos y apoyo económico

Un plugin del catálogo puede conectarse a un servicio de pago si su función principal tiene una alternativa local o autoalojable, y si lo declara claramente en su ficha. Donaciones y patrocinios al autor están permitidos y visibles. Se acepta que un plugin del registro pueda venderse en otro lugar: la licencia lo permite y combatirlo generaría conflicto con la comunidad.

### Marca e insignias

- El nombre, el logo y la designación "oficial" se rigen por un TRADEMARKS.md. Un fork puede existir, pero con otro nombre.
- Para hacer cumplir la política con fuerza, la marca debe registrarse (ver F). Sin registro solo queda una protección más débil y variable según el país.
- La política de marca no debe restringir el uso del software: solo cómo se presenta.

### Política propuesta: tres niveles verificables

| Nivel | Quién lo mantiene | Requisitos | Insignia |
| --- | --- | --- | --- |
| Oficial | El proyecto | Todos los de arriba + revisión del equipo central | "Oficial" |
| Verificado | Terceros | Todos los de arriba + revisión de seguridad | "Verificado" |
| Externo | Cualquiera | Ninguno; no aparece en el catálogo | Ninguna |

Proceso: solicitud pública por pull request al repositorio del registro, checklist automático, revisión humana, decisión razonada. El retiro siempre lleva aviso, motivo escrito y derecho a apelar.

## C. Arquitectura técnica

La arquitectura gira alrededor de un modelo de proyecto abierto. La geometría, los formatos y la IA son servicios que los módulos usan, no el centro.

### Capas

1. **Modelo de proyecto y formato de archivo.** Proyectos, ambientes, gabinetes, piezas, materiales, herrajes, reglas de construcción, planos y revisiones. Documentado y versionado.
2. **Runtime mínimo.** Carga de módulos, permisos, unidades (milímetros y pulgadas fraccionales), deshacer/rehacer, configuración e idiomas.
3. **Servicios compartidos.** Geometría 2D/3D sobre una librería existente, motor de reglas paramétricas, lectura y escritura DXF y PDF.
4. **Módulos.** Diseñador de gabinetes, despiece y optimización, shop drawings, estimación, CNC, revisión de planos, CAD 2D, IA.

### Núcleo o extensión

| En el núcleo (siempre instalado) | Como extensión (opcional) |
| --- | --- |
| Modelo de proyecto y formato | Todo módulo funcional de la capa 4 |
| Runtime de módulos y permisos | Soporte DWG (por licencia y tamaño) |
| Unidades y precisión | Vista 3D |
| Deshacer/rehacer, idiomas | Postprocesadores CNC por máquina |
| Visor 2D básico | IA, colaboración en línea |

Regla: entra al núcleo solo lo que al menos dos módulos necesitan y que no puede vivir fuera sin duplicarse.

### API de plugins estable

- Versionado semántico propio para la API, separado del de la aplicación.
- Superficie pequeña: los plugins hablan con el modelo de proyecto y con servicios públicos, nunca con internos.
- Manifiesto por plugin con permisos explícitos (archivos, red, ejecución de procesos).
- La API se mantiene en 0.x hasta que tres módulos reales la usen; recién entonces se declara 1.0.
- Toda función obsoleta avisa durante una versión mayor completa antes de desaparecer.
- La CI prueba cada cambio contra plugins de ejemplo para detectar rupturas.

Decisión abierta con efecto legal (ver A): plugins en el mismo proceso, o aislados (proceso separado o sandbox WASM). El aislamiento mejora la seguridad, pero debilita el argumento de que los plugins deben ser GPL. Se resuelve en un RFC.

### Distribución y equipos modestos

Un instalador base pequeño más un gestor que descarga módulos firmados, con opción de instalación sin conexión desde archivo. "Equipo modesto" se define con requisitos medibles (memoria, tiempo de arranque) que la CI verifica. Las cifras se fijan en un ADR. El 3D y la IA son opcionales para no imponer su costo.

### Riesgos técnicos

| Riesgo | Mitigación |
| --- | --- |
| Construir tres productos a la vez | Secuencia por fases; un solo módulo en curso hasta su primera versión usable |
| Escribir un motor geométrico propio | Reutilizar librerías maduras compatibles con GPLv3 |
| Fidelidad DWG | DXF primero; DWG como módulo separado |
| Errores de precisión y unidades | Decisión temprana en ADR, con pruebas en pulgadas fraccionales |
| API congelada demasiado pronto | Mantener 0.x hasta tener módulos reales que la validen |
| Rendimiento 3D en equipos modestos | 2D primero; 3D opcional |
| Costos de IA | Modelos locales e intercambiables; nunca requisito |

### Secuencia de desarrollo

1. Modelo de proyecto + diseñador de gabinetes básico + despiece y lista de materiales + exportación CSV, PDF y DXF.
2. Shop drawings y optimización de cortes.
3. Visor y marcado de PDF (revisión de planos).
4. Salida CNC.
5. CAD 2D propio y colaboración en servidor.

La IA entra de forma transversal cuando haya datos y módulos donde aporte. El lenguaje y el stack se deciden en un ADR durante la Fase 1.

## D. Gobernanza y comunidad

El proyecto arranca con un fundador-mantenedor que decide con reglas públicas, y con un disparador escrito para pasar a un consejo técnico.

### Roles

| Rol | Cómo se llega | Qué puede hacer |
| --- | --- | --- |
| Contribuidor | Primer aporte aceptado (código, docs, traducción, pruebas) | Proponer cambios y RFCs |
| Revisor | Aportes sostenidos en un área + nominación | Aprobar pull requests en su área |
| Responsable de módulo | Revisor sostenido + acuerdo de mantenedores | Decidir y publicar versiones de su módulo |
| Mantenedor central | Elegido por los mantenedores | Cambios en núcleo y API, versiones de la suite |
| Fundador | — | Desempate final, solo hasta que exista el consejo técnico |

Los criterios exactos (cantidad de aportes, tiempo) se fijan en GOVERNANCE.md y se publican antes de nombrar a nadie.

### Contribución y revisión

1. Issue o discusión pública.
2. Pull request con DCO firmado y CI en verde.
3. Una aprobación para cambios normales; dos para núcleo, API o licencias.
4. Cambios grandes (API, formato de archivo, nuevos módulos oficiales, gobernanza) pasan por un RFC con período público de comentarios.

### Resolución de conflictos

- **Técnicos:** discusión en el issue → decide el responsable del módulo → apelación al fundador o, después, al consejo técnico. Toda decisión queda escrita con su motivo.
- **De conducta:** se rigen por el código de conducta, en un canal separado. Mientras el fundador sea el único mantenedor, conviene sumar pronto una segunda persona de confianza para no ser juez y parte.

### Continuidad y no depender de una persona

- Dos propietarios de la organización de GitHub desde el inicio, con 2FA y códigos de recuperación guardados.
- Dominio, correo y cuentas registrados a nombre del proyecto; cuando sea posible, de un fiscal host.
- Proceso de publicación documentado, de modo que otra persona pueda sacar una versión.
- Disparador para el consejo técnico: por ejemplo, tres mantenedores activos, idealmente de organizaciones distintas.
- Plan de sucesión en GOVERNANCE.md: quién recibe accesos y responsabilidades si el fundador se retira.
- A largo plazo, evaluar una fundación o entidad paraguas que custodie la marca y los fondos.

## E. GitHub e infraestructura

Una organización de GitHub con pocos repositorios y controles de seguridad activos desde el día uno; lo demás espera a que haya comunidad.

### Repositorios iniciales

- **Repositorio principal (monorepo):** aplicación, módulos oficiales, especificación del formato, documentación, ADRs y RFCs.
- **Repositorio .github:** plantillas y archivos comunes para toda la organización.

Más adelante se separan el sitio web, el registro de plugins y la plantilla de plugin.

### Ahora o después

| Elemento | Ahora | Puede esperar |
| --- | --- | --- |
| Organización con 2FA obligatorio y dos propietarios | X | |
| README, LICENSE, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY | X | |
| GOVERNANCE, ROADMAP y TRADEMARKS en borrador | X | |
| Plantillas de issues y pull requests | X | |
| Rama principal protegida, pull request obligatorio, verificación DCO | X | |
| Secret scanning con push protection, Dependabot, reporte privado de vulnerabilidades | X | |
| ADRs de las decisiones de este documento | X | |
| CI con pruebas y análisis estático | Al elegir el stack | |
| Versionado semántico y notas de versión | Desde la primera versión | |
| Firma de binarios y paquetes | | Al publicar binarios |
| CODEOWNERS y equipos por módulo | | Con el segundo mantenedor |
| Sitio web, foro o chat | | Con usuarios reales |
| Registro de plugins | | Con la API 1.0 |
| Fiscal host y marca registrada | | Con donaciones o tracción |

### Correo, dominio, cuentas y secretos

- **Dominio propio** en cuanto se fije el nombre, con renovación automática y acceso de dos personas.
- **Correo del proyecto** con alias por función (seguridad, conducta, contacto) que reenvían a los responsables. Las personas cambian; los alias quedan.
- **Nunca compartir contraseñas personales.** Cada persona entra con su propia cuenta y permisos mínimos. Las credenciales del proyecto viven en un gestor de contraseñas con bóveda compartida.
- **Secretos de publicación** solo en entornos protegidos de GitHub Actions, con aprobación humana para publicar.
- **Cuentas de servicio** (registro de paquetes, correo, dominio) a nombre del proyecto, con al menos dos administradores y métodos de recuperación documentados.

## F. Riesgos legales y de propiedad intelectual

Inspirarse en las funciones de un producto es legal en general; copiar su código, recursos, documentación o marcas no lo es. El mayor riesgo concreto es el contrato laboral del fundador.

### Inspiración funcional frente a copia

| Sí se puede | No se debe |
| --- | --- |
| Implementar funciones equivalentes (capas, cotas, despiece) | Copiar código, íconos, textos de ayuda, plantillas o bibliotecas |
| Usar convenciones comunes de la industria | Clonar la interfaz pixel a pixel |
| Leer y escribir formatos por interoperabilidad | Hacer ingeniería inversa de binarios cuya licencia de uso lo prohíbe |
| Mencionar productos de forma descriptiva ("exporta DXF") | Usar sus marcas en el nombre, logo o dominio del proyecto |

La protección de interfaces y menús varía según el país. Por eso la regla práctica es inspirarse en el flujo de trabajo, no en el aspecto.

### Riesgos específicos

| Riesgo | Medida | ¿Requiere abogado? |
| --- | --- | --- |
| Cláusulas de cesión de PI o no competencia en el contrato laboral | No usar tiempo, equipos, archivos ni datos del empleador; pedir excepción escrita si la cláusula es amplia | Sí, antes de publicar |
| Marcas de terceros | Nombre propio verificado; uso solo descriptivo de marcas ajenas | Recomendable al elegir nombre |
| Patentes sobre procesos (por ejemplo, optimización de cortes) | Investigar antes de implementar esos módulos en mercados con patentes de software | Sí, para esos módulos |
| Catálogos de fabricantes de herrajes y materiales | Usar datos propios o con permiso explícito | Según el caso |
| Datos personales en servicios de servidor | Política de privacidad; atención a leyes de los países de los usuarios | Sí, antes de lanzar servicios |

### Desarrollo independiente documentado

- Política escrita de "sala limpia": prohibido aportar código, recursos o documentación de productos propietarios.
- Cada decisión técnica relevante registra sus fuentes públicas en un ADR (especificaciones, papers, libros).
- Archivos de prueba creados por el proyecto, nunca planos de clientes o empleadores.
- DCO en cada commit y encabezados SPDX en cada archivo, verificados en la CI.

### Bolivia y colaboradores internacionales

- La marca se registra por país o región. En Bolivia corresponde a SENAPI; otros mercados requieren evaluación aparte.
- La Decisión 486 de la Comunidad Andina excluye de patente a los programas de ordenador como tales. Eso no protege frente a patentes en otros países donde se distribuya el software. Verificar con un especialista.
- Recibir donaciones del exterior puede tener efectos tributarios y cambiarios en Bolivia. Consultar con un contador antes de abrir canales de financiación; un fiscal host extranjero puede simplificarlo.
- Con el DCO, cada colaborador declara tener derecho a aportar su código bajo la licencia del proyecto, sin importar su país.

## G. Hoja de ruta

Seis fases, cada una con una puerta de salida verificable. No hay fechas: el ritmo lo marca el tiempo disponible del fundador y la llegada de colaboradores.

Resumen de puertas:

- Fase 0 → 1: abogado consultado y nombre libre.
- Fase 1 → 2: RFCs aceptados; el prototipo guarda y reabre un proyecto.
- Fase 2 → 3: usuarios reales producen un despiece correcto.
- Fase 3 → 4: segundo mantenedor y un plugin externo funcionando.
- Fase 4 → 5: API 1.0 estable con tres módulos que la usan.

### Fase 0 — Fundamentos

- **Objetivo:** cerrar los riesgos que impiden publicar.
- **Entregables:** revisión legal del contrato laboral; nombre verificado; misión y definición de "gratuita" escritas.
- **Decisiones cerradas:** licencias por componente; DCO; nombre.
- **Para avanzar:** abogado consultado y nombre libre en GitHub, dominio y registros de marcas.

### Fase 1 — Entorno y diseño

- **Objetivo:** un repositorio profesional y las decisiones técnicas que no se pueden revertir fácil.
- **Entregables:** organización y archivos de la sección E; RFC del modelo de proyecto; RFC de la arquitectura de módulos; ADR del stack y de unidades; prototipo técnico.
- **Decisiones cerradas:** stack, unidades y precisión, plugins en proceso o aislados.
- **Para avanzar:** RFCs aceptados y el prototipo guarda y reabre un proyecto en el formato propio.

### Fase 2 — Primer módulo útil

- **Objetivo:** una herramienta que un taller use en trabajo real.
- **Entregables:** diseñador de gabinetes básico, despiece, lista de materiales y exportación CSV, PDF y DXF. Versión 0.1 publicada.
- **Decisiones cerradas:** alcance de la 0.1 y formato de archivo versión 1 en borrador.
- **Para avanzar:** usuarios reales, ajenos al empleo del fundador, producen un despiece correcto de un proyecto real.

### Fase 3 — Comunidad temprana

- **Objetivo:** dejar de depender de una sola persona.
- **Entregables:** guía de contribución probada, issues para principiantes, API de plugins 0.x documentada, primer plugin externo.
- **Decisiones cerradas:** criterios para revisores y mantenedores; canal de comunidad.
- **Para avanzar:** un segundo mantenedor activo y un plugin externo funcionando.

### Fase 4 — Expansión

- **Objetivo:** cubrir el flujo completo de millwork y abrir el ecosistema.
- **Entregables:** shop drawings, optimización de cortes, visor y marcado de PDF, registro de plugins verificados, fiscal host, marca registrada.
- **Decisiones cerradas:** API 1.0 y política del registro definitiva.
- **Para avanzar:** API 1.0 estable con tres módulos que la usan.

### Fase 5 — Plataforma

- **Objetivo:** la suite completa y una gobernanza colectiva.
- **Entregables:** CAD 2D propio, salida CNC, colaboración en servidor (AGPLv3), IA local opcional.
- **Decisiones cerradas:** consejo técnico constituido.
- **Para avanzar:** el proyecto funciona sin que el fundador sea indispensable.

## Decisiones pendientes y asesoría jurídica

### Decisiones a cerrar (Fases 0 y 1)

- [ ] Nombre del proyecto, verificado en GitHub, dominio y registros de marcas
- [x] GPL-3.0-or-later o GPL-3.0-only (ADR-0013: "or later")
- [ ] Plugins en el mismo proceso o aislados
- [x] Lenguaje y stack técnico (ADR-0011: Rust; ADR-0012: Slint)
- [ ] Modelo de unidades y precisión
- [ ] Segundo propietario de la organización de GitHub
- [ ] Definición final de "gratuita"

### Consultas para un abogado de propiedad intelectual

- [ ] Contrato laboral: cesión de PI, no competencia, confidencialidad
- [ ] Registro de marca en Bolivia y en los mercados principales
- [ ] Riesgo de patentes antes de los módulos de optimización y CNC
- [ ] Revisión de TRADEMARKS.md y de la política del registro de plugins
- [ ] Privacidad y datos personales antes de lanzar servicios de servidor
- [ ] Con un contador: recepción de donaciones desde el exterior

Las afirmaciones legales de este documento son orientativas, basadas en conocimiento general a la fecha. Deben verificarse con fuentes jurídicas actuales y asesoría en cada jurisdicción relevante.

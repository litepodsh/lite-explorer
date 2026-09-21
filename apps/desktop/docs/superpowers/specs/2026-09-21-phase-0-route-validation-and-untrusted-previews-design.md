# Fase 0: rutas locales y previews no confiables

## Objetivo

Reducir el riesgo de que contenido o rutas no confiables hagan que Lite Explorer
lea, escriba, borre, abra o previsualice una ubicación distinta de la elegida.
La fase cubre rutas **locales**. S3, SMB, WebDAV, NFS, SFTP y FTP conservan su
validación específica y no se modifican aquí.

## Alcance

1. Centralizar la validación de rutas locales en Rust.
2. Aplicarla a crear, renombrar, copiar, mover, borrar, enviar a papelera,
   extraer, buscar y leer previews locales.
3. Tratar todo preview como contenido no confiable.
4. Exigir confirmación explícita para abrir un enlace externo desde un preview.
5. Reducir el permiso `opener` comodín y conservar la apertura externa detrás
   de comandos propios validados.

No se añade un modelo de tokens por panel, sandbox de procesos, soporte de
pausar/reintentar ni autorización de URIs remotas. Son fases posteriores.

## Validación de rutas

Un módulo compartido de `explorer` será la única puerta para las rutas locales
en comandos sensibles.

- Una ruta existente debe ser absoluta y se inspecciona con `symlink_metadata`.
  Si ella misma, o cualquier componente ya existente de su recorrido, es un
  symlink, se rechaza.
- Una ruta nueva se forma únicamente como `parent / nombre`. El `parent` debe
  existir, ser un directorio absoluto sin symlinks, y `nombre` debe ser un
  único nombre de archivo: no vacío, distinto de `.` y `..`, sin `/` ni `\\`.
- Copiar y mover validan el origen existente y el directorio destino antes de
  planificar objetivos. El nombre reservado se crea bajo ese directorio; nunca
  se acepta un objetivo completo aportado por el frontend.
- Borrar, papelera, previews y búsqueda validan la ruta existente antes de
  actuar. Extracción valida archivo y directorio destino antes de crear cada
  objetivo.
- Los errores hacia la UI son constantes y no incluyen rutas, credenciales ni
  detalles del sistema. Los logs de depuración tampoco registran rutas en los
  comandos destructivos.

La aplicación sigue siendo un explorador local: una ruta válida no queda
limitada a una raíz global. Este guard evita traversal y seguimiento de enlaces
simbólicos; un modelo de autorización por panel requeriría tokens emitidos por
el backend y queda fuera de esta fase.

## Previews no confiables

- El preview HTML se presenta en un iframe sin `allow-scripts` ni
  `allow-popups`; no puede ejecutar JavaScript, navegar ni enviar formularios.
- Markdown, EPUB, correo y HTML no abren enlaces automáticamente. Un clic
  muestra el destino y una advertencia; solo la confirmación del usuario llama
  al comando de apertura externa.
- El diálogo rechaza esquemas peligrosos y solo permite URLs `http` o `https`.
  Los recursos locales relativos continúan sin cargarse desde previews.
- La CSP de producción actual se conserva y se ajusta únicamente si las
  pruebas prueban una dependencia necesaria. No se añaden excepciones de red.

## Permisos Tauri

Las capabilities `main` y `viewer` dejan de conceder
`opener:allow-open-path` con `"**"`. La interfaz usa un comando de backend que
valida la ruta local o la URL confirmada y abre con argumentos separados. La
ventana `settings` no recibe permisos de apertura.

## Flujo y fallos

1. La UI solicita una operación con la selección y, para operaciones de hijo,
   un nombre simple.
2. El comando distingue remoto de local. La rama local pasa primero por el
   guard compartido.
3. Si la validación falla, no se inicia trabajo ni se modifica el filesystem.
4. Si pasa, el código existente ejecuta la operación coordinada y devuelve un
   error general si falla.

Un cambio de filesystem entre validación y operación sigue siendo posible en
los sistemas de archivos normales. Esta fase vuelve a comprobar justo antes de
la acción compartida y bloquea symlinks; una garantía libre de TOCTOU exige
handles de directorio y APIs específicas por plataforma, fuera de este alcance.

## Pruebas de aceptación

- Crear y renombrar rechazan vacío, `.`, `..`, `/`, `\\` y nombres que intenten
  salir del padre.
- Copiar, mover, borrar, papelera, búsqueda, preview y extracción rechazan
  rutas relativas y rutas que recorran un symlink.
- Una extracción no puede escribir fuera del destino ni atravesar un symlink
  creado dentro del árbol de destino.
- Un HTML con `script`, popup, formulario o navegación no ejecuta ni abre nada.
- Un enlace `https` en Markdown, EPUB o correo no abre nada antes de aceptar el
  aviso; al aceptar usa el abridor validado.
- Las capabilities no contienen `opener:allow-open-path` con `"**"`.

## Verificación

Se añaden pruebas Rust pequeñas para el guard de rutas y pruebas de componente
para la confirmación de enlaces. Se ejecutan las pruebas afectadas, el chequeo
de Rust y el build del frontend.

## Fuera de alcance y siguiente fase

La Fase 1 unifica conflictos, cancelación, reintentos y estados parciales para
copiar, mover y borrar; después añade propiedades/permisos y filtros de
búsqueda. No se mezclan con este cambio de seguridad.

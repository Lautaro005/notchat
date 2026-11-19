# AGENT.md — instrucciones del proyecto Notchat

Leé este archivo completo antes de construir o cambiar cualquier cosa. Si un pedido contradice una regla de acá, avisá y preguntá antes de seguir.

## Qué es

**Notchat**: cliente de WhatsApp no oficial para macOS con IA local. Liviano, rápido, minimalista. Notificaciones en el notch con respuesta inline. Se instala con un solo comando desde GitHub.

## Decisiones cerradas (no reabrir sin que el usuario lo pida)

| Tema | Decisión |
|---|---|
| Stack | Rust (núcleo) + **Tauri v2** + UI en **Svelte 5 + Vite + TypeScript** |
| Gestor de paquetes JS | `npm` (no pnpm, no bun) |
| Plataforma | macOS, Apple Silicon primero; build universal en release |
| Nombre | **Notchat** (nunca usar "WhatsApp"/"Whats" en nombre, bundle id ni ícono) |
| Idioma | UI en español e inglés según el sistema; README en inglés + `README.es.md` |
| Firma | Ad-hoc por ahora; el pipeline queda listo para Developer ID + notarización |
| Licencia | MIT |
| WhatsApp | crate `whatsapp-rust`, versión exacta fijada |
| IA | Sólo proveedores locales, un único cliente OpenAI-compatible |

## Estructura

```
Cargo.toml            workspace
crates/core/          lógica pura, sin dependencias de Tauri, testeable
  wa/                 sesión, eventos, envío, QR
  store/              SQLite cifrado (SQLCipher)
  ai/                 proveedores, detección, llama.cpp gestionado
  agent/              loop de tool-calling y herramientas
  policy/             reglas de auto-respuesta
src-tauri/            commands, notch, notificaciones, tray
ui/                   frontend
install.sh            instalador de un comando
docs/security/        threat-model.md, controls.md
```

Regla: la lógica va en `crates/core`; `src-tauri` sólo conecta. Nada de lógica de negocio en el frontend.

## Funcionalidad requerida

- **Vinculación**: QR generado en Rust (`qrcode` → SVG) y enviado a la UI por evento; sin servicios externos. También pair-code.
- **Proveedores de IA**: detectar lo instalado (Ollama `:11434`, LM Studio `:1234`, llama.cpp / Llama.app `:8080`) y dejar elegir proveedor y modelo. Si no hay nada, botón que instala `llama-server` (release fijado + SHA-256) y descarga un modelo liviano elegido de `models.json`.
- **Chat "Notchat AI"**: chat propio, no de WhatsApp, fijado arriba. Acepta adjuntos. Herramientas: `list_chats`, `read_chat`, `search_messages`, `send_message`, `web_search`, `fetch_url`, `read_attachment`.
- **Enviar por pedido del usuario**: `send_message` muestra tarjeta de confirmación (destinatario + texto). El envío sin confirmar es un toggle, apagado por defecto.
- **Auto-respuesta**: Todos / Seleccionados / Ninguno, listas separadas para chats y grupos, opción "sólo si me mencionan" en grupos, instrucciones por chat, rate limit, horario, kill-switch global. **Apagada por defecto.**
- **Notch**: `NSPanel` no activante con respuesta inline y sugerencia de IA; píldora superior en Macs sin notch. Sólo APIs públicas de macOS.
- **Notificaciones**: toggle Notch ↔ Sistema; las del sistema con acción de respuesta de texto.

## Reglas de seguridad (obligatorias)

1. **Todo local.** Sin telemetría, sin analytics, sin servidores propios. La única red permitida: WhatsApp (incluido su CDN de fotos de perfil y archivos), el proveedor de IA en `localhost`, web cuando el usuario lo pide en el chat propio, y GitHub para buscar e instalar actualizaciones (interruptor en Ajustes, prendido por defecto; decidido por el usuario en 0.1.2).
2. **Mensajes entrantes = datos no confiables.** En auto-respuesta la IA sólo ve ese chat y **no tiene herramientas**: ni internet, ni otros chats, ni envío a terceros. Las herramientas sólo existen en el chat "Notchat AI".
3. **Secretos en Keychain**, nunca en archivos, logs ni en el frontend. La DB va cifrada con SQLCipher; archivos con permisos 0600.
4. **Nunca loguear** contenido de mensajes, números, ni claves. Los logs llevan ids y conteos.
5. **Tauri**: CSP estricta, sin contenido remoto, capabilities mínimas por ventana, validar toda entrada en cada command.
6. **`fetch_url`** bloquea IPs privadas, loopback y link-local (SSRF) y limita tamaño y tiempo.
7. **Descargas** (llama-server, modelos, releases): siempre con verificación SHA-256 antes de usar.
8. **Dependencias**: versiones fijadas, `Cargo.lock` y `package-lock.json` en el repo. Dependencia nueva = justificarla; preferir la que ya está.
9. **No afirmar certificaciones.** El software no está "certificado ISO 27001/SOC 2". Se documenta el mapeo de controles como "alineado, no certificado".
10. **Avisos honestos**: README y onboarding dicen que es no oficial, no afiliado a Meta, y que automatizar puede provocar el baneo del número.

## Diseño

- Minimalista: tipografía del sistema, mucho aire, un solo color de acento, tokens CSS para claro/oscuro.
- Sin frameworks de UI pesados ni librerías de componentes. CSS propio con variables.
- Animaciones cortas y discretas; respetar `prefers-reduced-motion`.
- Accesible: contraste AA, navegación por teclado, etiquetas en controles.
- Todo texto visible pasa por i18n (ES/EN); nada hardcodeado.

## Rendimiento

- Arranque en frío rápido: nada de trabajo pesado en el hilo principal ni al iniciar.
- Listas de chats y mensajes virtualizadas; paginar desde SQLite.
- Streaming de tokens de la IA por eventos, sin bloquear la UI.
- Binario chico: evitar dependencias grandes, `opt-level`/LTO/strip en release.

## Calidad

Antes de dar algo por terminado:

```
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
npm --prefix ui run check
```

- Tests unitarios para `policy`, `agent` (con proveedor falso), `store` y el guard SSRF.
- Sin `unwrap()`/`expect()` fuera de tests; errores con tipos (`thiserror`).
- `unsafe` sólo en el puente con AppKit, acotado y comentado.
- Reportar el resultado real: si algo no compila o no se probó, decirlo.

## Notas operativas (aprendidas al construir)

- `whatsapp-rust` 0.7: usar `default-features = false` con `tokio-transport`, `tokio-runtime`, `ureq-client`. **No** activar `simd` (requiere nightly). El store de sesión se cifra con `SqliteStoreConfig::with_connection_init` + `PRAGMA key`; la DB de la app usa la misma clave.
- SQLite: una sola `libsqlite3-sys` en todo el árbol (`bundled-sqlcipher`). diesel 2.4 acepta `<0.41`. Antes de subir versiones correr `cargo deny check`.
- `tauri_panel!` trae sus propios `use`: va en un submódulo aparte (ver `src-tauri/src/notch.rs`).
- Desarrollo: `NOTCHAT_DEV_KEY` (64 hex) evita el prompt del Llavero; los datos de debug van a `.../app.notchat.desktop/dev/`. Compilar con `CARGO_TARGET_DIR` en un disco interno si el repo está en un disco externo lento.
- Vista previa del diseño sin la app: `npm --prefix ui run dev` y abrir `http://127.0.0.1:1420/?view=chats|pair|code|empty|noai|aiempty|auto` (datos de `ui/src/lib/mock.ts`, nunca en el build de producción).
- El panel del notch carga el mismo bundle con `index.html#notch` (`ui/src/main.ts`).
- Catálogo de modelos y build de `llama-server`: `models.json` (se embebe al compilar). Para actualizar, tomar `digest`/`lfs.oid` de la API de GitHub/Hugging Face; nunca inventar hashes. Test real de descarga: `cargo test -p notchat-core -- --ignored`.
- Notificaciones del sistema solo funcionan desde el `.app` empaquetado (necesitan bundle id); en `npm run dev` se usa el notch.
- El proceso fija `umask 077`: todo archivo nuevo queda `0600`.
- Historial: `skip_history_sync` ya no se usa; los chats, grupos y mensajes del teléfono se importan en `wa::import_history`. Los contactos van a la tabla `contacts` y no crean chats; la lista solo muestra chats con `last_ts > 0` o fijados.
- Archivos y fotos de perfil: claves de descarga en la columna `messages.media`; los bytes descargados en `media/` y `avatars/` con nombres hasheados (`crates/core/src/media.rs`).
- Acciones de mensaje (responder, reaccionar, editar, borrar, destacar, fijar, reenviar): `WaClient` en `wa.rs`; el mensaje completo se guarda codificado en `messages.raw` para poder citarlo o reenviarlo sin volver a subir el archivo. Reacciones en la tabla `reactions` (la propia con remitente `me`).
- Stickers: tabla `stickers` (id = SHA-256 del archivo); se llena con `recentStickers` del historial y con cada sticker enviado o recibido. Los Lottie se ignoran.
- Notas: tabla `notes`; sus archivos van a `notes/` (aparte de `media/`, así «Liberar espacio» nunca los borra).
- Nombres: la vista temporal `people` busca un contacto por número o por LID (tabla `lid_map`, que se llena con `sender_alt`/`recipient_alt`, el historial, los contactos y los participantes de grupos). Orden: nombre de la agenda → `~` + nombre de WhatsApp → `+número`. Se resuelven al leer (`resolve_people` en `store.rs`), nunca se guardan ya resueltos.
- Menciones: `messages.mentions` guarda los JID mencionados; al leer se devuelven como `Person` para que la UI muestre `@Nombre`.
- Leídos en el teléfono: los recibos `read-self` bajan el contador con `Store::mark_read_up_to`.
- Puertos de los proveedores locales: ajuste `ai_ports` (`ProviderPorts`), siempre en 127.0.0.1 y ≥ 1024.
- Vista previa del notch en el navegador: `http://127.0.0.1:1420/?view=notch#notch`.
- Actualizaciones: `crates/core/src/update.rs` (verifica `SHA256SUMS` del release) y `src-tauri/src/updater.rs` (reemplaza el `.app` al salir).

## Acciones que requieren confirmación del usuario

- Crear el repo en GitHub, hacer push, publicar releases.
- Enviar mensajes reales de WhatsApp durante pruebas.
- Cambiar cualquiera de las decisiones cerradas.

## Fases

1. Base: workspace, store cifrado, QR, chats, leer/enviar texto.
2. IA: detección, ajustes, chat propio con streaming y herramientas.
3. Auto-respuesta: políticas, UI, límites, kill-switch.
4. Notch y notificaciones.
5. llama.cpp gestionado y catálogo de modelos.
6. Seguridad documentada, CI, `install.sh`, README, release.

Cada fase termina compilando y probada antes de empezar la siguiente.

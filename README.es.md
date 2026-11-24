<div align="center">

<img src="docs/assets/icon.png" width="112" height="112" alt="Ícono de Notchat">

# Notchat

**WhatsApp en tu Mac, con una IA privada que vive en el notch.**

Liviano, rápido y local. Escrito en Rust.

[![CI](https://github.com/Lautaro005/notchat/actions/workflows/ci.yml/badge.svg)](https://github.com/Lautaro005/notchat/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Lautaro005/notchat?label=release)](https://github.com/Lautaro005/notchat/releases)
[![Licencia: MIT](https://img.shields.io/badge/licencia-MIT-black)](LICENSE)
![macOS 13+](https://img.shields.io/badge/macOS-13%2B-black)

[Instalar](#instalar) · [Funciones](#funciones) · [Privacidad](#privacidad-y-seguridad) · [English](README.md)

</div>

---

## Instalar

```bash
curl -fsSL https://raw.githubusercontent.com/Lautaro005/notchat/main/install.sh | sh
```

El script descarga la última versión, la compara con el SHA-256 publicado y deja `Notchat.app` en `/Aplicaciones`. ¿Preferís compilarla vos?

```bash
curl -fsSL https://raw.githubusercontent.com/Lautaro005/notchat/main/install.sh | sh -s -- --source
```

Después abrí Notchat y en el teléfono andá a **WhatsApp → Ajustes → Dispositivos vinculados → Vincular un dispositivo** y escaneá el código. El QR se genera dentro de la app; no pasa por ningún tercero.

> [!NOTE]
> Por ahora las versiones se firman ad-hoc (todavía sin notarizar por Apple). El instalador verifica el checksum; el pipeline ya está listo para firmar con Developer ID.

## Funciones

|  |  |
|---|---|
| **Respuestas desde el notch** | Los mensajes nuevos bajan desde el notch. Tocás *Responder*, escribís y listo, sin dejar lo que estabas haciendo. ¿Preferís las notificaciones del sistema? Se cambia en Ajustes; las dos permiten responder. |
| **Notchat AI** | Un chat propio con tu asistente. Lee tus chats, busca en internet, lee los archivos que adjuntás y manda mensajes por vos: *"decile a Ana que llego tarde"*. Siempre confirmás antes de que se envíe. |
| **Respuesta automática** | La IA puede contestar todos los chats, los que elijas o ninguno, por separado para chats y grupos, con instrucciones por chat, límite por hora, horario de silencio y un botón de pausa en la barra de menú. |
| **Usá tu propio modelo** | Funciona con lo que ya tengas: Ollama, LM Studio o llama.cpp. ¿No tenés nada? Con un botón descarga llama.cpp y un modelo liviano (Gemma 4, Qwen3.5) y lo corre en tu Mac. |
| **La app de chat completa** | Tus chats, grupos e historial del teléfono, con fotos de perfil. Fotos, videos, audios (grabar y escuchar), documentos, encuestas (ver resultados y votar), arrastrar archivos desde Finder directo al chat y emoji. Fijá, silenciá, archivá, marcá como no leído, exportá y vaciá chats desde el clic derecho; los chats archivados no notifican. |
| **Todas las acciones de mensaje** | Responder (con el mensaje citado a la vista), reaccionar (clic en una reacción para ver quién la dejó), destacar, fijar, reenviar, copiar, editar, eliminar para todos, guardar en Notas y seleccionar varios. Mandá los stickers de tu cuenta. |
| **Notas** | Un chat privado para guardar textos, archivos y mensajes, como chat, tablero o post-its. No llega a WhatsApp salvo que reenvíes algo. |
| **Tus propias listas** | Agrupá chats en listas como *Trabajo* o *Familia* y filtrá por ellas, junto a *No leídos* y *Grupos*. La lista de chats se puede ensanchar a gusto. |
| **Liviano** | Núcleo nativo en Rust, webview del sistema e interfaz de ~90 KB. Corre en segundo plano desde la barra de menú. |
| **Bilingüe** | Español e inglés, según el idioma del sistema. |

## Privacidad y seguridad

- **Todo queda en tu Mac.** Sin servidores de Notchat, sin cuentas, sin telemetría. La IA corre en `localhost`; los endpoints remotos se rechazan.
- **Cifrado en disco** con SQLCipher (AES-256); la clave vive en tu Llavero.
- **Protegida contra prompt injection.** La respuesta automática solo ve el chat que contesta y no tiene herramientas. El asistente siempre pregunta antes de enviar si leyó chats o páginas web.
- **Descargas verificadas.** llama.cpp y los modelos están fijados por SHA-256; las actualizaciones se comparan con el SHA-256 publicado antes de instalarse.
- **Actualizaciones a tu manera.** Notchat busca una versión nueva en GitHub al abrir y cada tres horas; se apaga en Ajustes → Actualizaciones.
- **Cadena de suministro auditada.** `cargo audit`, `cargo deny`, CodeQL, SBOM y procedencia firmada en cada versión.

Leé la [política de seguridad](SECURITY.md), el [modelo de amenazas](docs/security/threat-model.md) y el [mapeo de controles ISO 27001 / OWASP ASVS](docs/security/controls.md). Notchat está alineado con esos marcos; al ser software, no está certificado en sí mismo.

## Requisitos

- macOS 13 Ventura o posterior, Apple Silicon o Intel
- Una cuenta de WhatsApp en tu teléfono
- Para la IA: 8 GB de RAM (16 GB para los modelos más grandes)

## Compilar

```bash
git clone https://github.com/Lautaro005/notchat.git && cd notchat
npm run setup
npm run dev      # desarrollo con recarga en caliente
npm run build    # Notchat.app en target/release/bundle/macos
```

Requiere Rust estable, Node.js 22+ y las Xcode Command Line Tools. Ver [CONTRIBUTING.md](CONTRIBUTING.md).

## Aviso

Notchat es un cliente independiente y **no oficial**. No tiene relación con WhatsApp ni con Meta, ni cuenta con su aval. "WhatsApp" es una marca de su titular. Usar clientes no oficiales, y en especial automatizar respuestas, puede hacer que WhatsApp restrinja o suspenda tu número. Usalo bajo tu responsabilidad.

## Licencia

[MIT](LICENSE)

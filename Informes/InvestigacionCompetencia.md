# Investigación de competencia y sitios de publicación — tubetui

Fecha: 2026-11-01 · Tarea #78

**Cómo se hizo:** las estrellas, fechas de último push y estado de PRs salen de la API de GitHub (`gh`) del 2026-11-01. La búsqueda web no estuvo disponible, así que **las características de cada competidor son de mi conocimiento previo y de sus README; conviene verificarlas antes de usarlas en un post público.**

**Punto de partida:** tubetui tiene **72 estrellas** en GitHub. Hace falta ver cuánta de la competencia sigue activa.

---

## 1. Competencia (10)

| # | Proyecto | ★ | Último push | Link |
|---|---|---|---|---|
| 1 | youtube-tui (Siriusmart) | 1.178 | 2026-05 | https://github.com/Siriusmart/youtube-tui |
| 2 | yewtube (ex mps-youtube) | 8.804 | 2026-03 | https://github.com/mps-youtube/yewtube |
| 3 | ytfzf | 4.162 | 2024-09 (sin mantenimiento) | https://github.com/pystardust/ytfzf |
| 4 | YTerMusic | 703 | 2026-05 | https://github.com/ccgauche/ytermusic |
| 5 | youtube-viewer | 1.320 | 2026-06 | https://github.com/trizen/youtube-viewer |
| 6 | FreeTube | 22.290 | activo | https://github.com/FreeTubeApp/FreeTube |
| 7 | NewPipe | 40.396 | activo | https://github.com/TeamNewPipe/NewPipe |
| 8 | Piped | 10.300 | activo | https://github.com/TeamPiped/Piped |
| 9 | Invidious | 25.819 | activo | https://github.com/iv-org/invidious |
| 10 | spotify-player (referencia de TUI multimedia, no es YouTube) | 7.293 | 2026-09 | https://github.com/aome510/spotify-player |

### Qué tienen mejor que nosotros

**1. youtube-tui (Siriusmart)** — el competidor directo (Rust, mpv, TUI de YouTube).
- 16 veces más estrellas y más tiempo de mercado; está en crates.io y en AUR.
- Manual propio (tui.siri.ws/youtube) y configuración YAML muy personalizable (temas, atajos, layout).
- Soporta imágenes por Sixel además de kitty, así que funciona en más terminales.
- Tiene Linux y Windows desde hace tiempo, más comunidad e issues resueltos.
- *Nosotros ganamos en:* cuenta/login, historial, suscripciones, voz traducida, comentarios, tray, instaladores.

**2. yewtube** — TUI en Python.
- Instalación trivial (`pipx install yewtube`) y fama de años (heredó mps-youtube).
- Funciona sin API key, con comandos tipo vim y descarga de audio/video, playlists locales y soporte de Last.fm/scrobbling (de memoria).
- Muy extendido en wikis y listas de distros.
- *Nosotros ganamos en:* UI moderna con ratón y miniaturas, cuenta de YouTube, reproducción embebida.

**3. ytfzf** — script POSIX con fzf.
- Ligerísimo (solo un script), sin compilar.
- Soporta muchos sitios además de YouTube (Invidious, PeerTube, etc.) y miniaturas en el selector.
- Muy fácil de empaquetar y de integrar en scripts.
- Ojo: el README dice que ya no tiene mantenimiento. Es una oportunidad: sus usuarios buscan reemplazo.

**4. YTerMusic** — reproductor de YouTube Music en TUI.
- Enfocado en música: caché local, funciona sin conexión con lo ya descargado, ~20 MB de RAM, descarga en segundo plano.
- Temas con colores hex y mejor integración con YouTube Music (supermix).
- *Nosotros:* hacemos video y música de archivos locales, pero la música de YouTube es menos específica.

**5. youtube-viewer (trizen)** — cliente en Perl con GTK y terminal.
- Años de trayectoria, versión GTK además de CLI, muy empaquetado en distros.
- Mucha opción de filtros de búsqueda y de API.

**6. FreeTube** — app de escritorio con privacidad.
- GUI completa para Windows/macOS/Linux, sin cuenta de Google: suscripciones locales, perfiles, historial.
- Comunidad grande (22 k★), instaladores en todos lados (Flathub, winget, brew), integración con SponsorBlock.
- Es lo que usa quien no quiere terminal. *Nosotros:* mejor para quien vive en la terminal y quiere sus suscripciones reales de YouTube.

**7. NewPipe** — app de Android.
- Referente de "YouTube sin Google", 40 k★, en F-Droid, descarga y reproducción en segundo plano.
- Plataforma móvil que nosotros no cubrimos. Se puede aprender de su claridad de mensaje (liviano, libre, sin cuenta).

**8. Piped** — frontend web con privacidad.
- Se usa desde cualquier navegador con instancias públicas, sin instalar nada.
- Cuenta propia, SponsorBlock integrado, eficiente por diseño.

**9. Invidious** — frontend web alternativo.
- 25 k★, instancias públicas, API propia que usan muchos otros proyectos (ytfzf, youtube-tui).
- Cuando YouTube bloquea, sus instancias a veces caen: riesgo para quienes dependen de ellas. Nosotros usamos yt-dlp directo.

**10. spotify-player** — TUI de Spotify "con paridad completa de funciones".
- Muestra qué se espera de una TUI multimedia madura: letras, cola, mpris, notificaciones, cover art, temas, instalación por todos los gestores (brew, AUR, winget, cargo).
- Es el estándar de pulido al que podemos apuntar, y vive en las mismas listas awesome que nosotros.

### Resumen: lo que más nos falta frente a todos
1. **Visibilidad y prueba social** (estrellas, paquetes en AUR/Homebrew/winget/Flathub, menciones).
2. **Soporte de más terminales** (Sixel/iTerm2, no solo kitty para el video embebido).
3. **Manual/documentación dedicada** y temas/atajos configurables al nivel de youtube-tui.
4. **Más plataformas de empaquetado** (AUR, Homebrew tap, Nix, Flatpak).
5. **Más tiempo de madurez**: tubetui tiene semanas (primer commit 2026-10-09).

---

## 2. Dónde publicar (5)

| # | Sitio | Link | Estado / nota |
|---|---|---|---|
| 1 | awesome-ratatui | https://github.com/ratatui/awesome-ratatui | ✅ **PR #496 aceptado** (2026-10-10). Hecho. |
| 2 | awesome-tuis | https://github.com/rothgar/awesome-tuis | ⛔ PR #935 **cerrado automáticamente**: el repo exige ≥6 meses desde el primer commit (el nuestro es del 2026-10-09). **Reintentar a partir del 2027-04-09.** |
| 3 | Reddit r/rust y r/commandline | https://www.reddit.com/r/rust/ · https://www.reddit.com/r/commandline/ | Mejor lugar para feedback real. Poner capturas (`docs/video.png`) y contar lo diferente (cuenta real + voz traducida). Leer las reglas de autopromoción de cada sub. |
| 4 | Show HN (Hacker News) | https://news.ycombinator.com/showhn.html | Título "Show HN: tubetui – YouTube in the terminal (Rust)". Mucho alcance si pega; hay que responder comentarios el mismo día. |
| 5 | Terminal Trove | https://terminaltrove.com/ (botón "Submit a tool") | Catálogo de herramientas TUI/CLI, con buena audiencia. |

Otros que quedan pendientes de la memoria del proyecto: hilo *Crate of the Week* (https://users.rust-lang.org/t/crate-of-the-week/2704) y anuncio en users.rust-lang.org. Alternativas a evaluar: lobste.rs (necesita invitación), r/unixporn, r/linux, This Week in Rust (ya no acepta PRs de proyectos propios).

**Qué tienen mejor los sitios frente a nuestra situación actual:**
- *awesome-tuis*: audiencia enorme (21 k★) pero su filtro de 6 meses nos deja fuera hasta abril de 2027.
- *awesome-ratatui*: ya conseguido; es de nicho (2 k★) pero sirve de backlink.
- *Reddit/HN*: son los que dan estrellas rápido, pero no se pueden repetir; conviene publicar cuando haya release pulida y capturas buenas.
- *Terminal Trove*: indexa por categorías y suele salir en búsquedas.

---

## 3. Recomendaciones

1. Publicar en **r/rust, r/commandline y Terminal Trove** primero (se puede hacer ya).
2. Subir paquetes a **AUR** y un **tap de Homebrew**: es lo que más falta frente a youtube-tui y spotify-player.
3. Apuntar a los usuarios de **ytfzf** (sin mantenimiento) y **yewtube**: "alternativa moderna con cuenta de YouTube".
4. Evaluar **Sixel** para el video/miniaturas fuera de kitty.
5. Anotar **2027-04-09** para reintentar el PR de awesome-tuis.
6. No publiqué ni abrí nada: los textos de los posts se muestran antes, como se pidió.

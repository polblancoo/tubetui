# Evaluación de Sixel (tarea #83)

## Miniaturas — ya resuelto en el árbol de trabajo (sin commitear)
`src/thumbs.rs` detecta el protocolo (`Graphics::{Kitty,Iterm2,Sixel}`): foot, mlterm,
contour, yaft o `TERM` con "sixel"; `TUBETUI_GRAPHICS=kitty|iterm2|sixel|none` fuerza uno.
Sixel se codifica en casa (256 colores 3-3-2, bandas de 6 px, `place_sixel`), sin
dependencias nuevas. Tests de detección y de codificación incluidos; `cargo check` y
`cargo test --lib` pasan. README y ajuste de configuración ya mencionan Sixel.
No se probó visualmente en un terminal Sixel real (no hay uno en esta sesión).

## Video — evaluación de `mpv --vo=sixel`
- mpv 0.41 local lista `--vo=sixel` (requiere libsixel en la compilación; no todas las
  distros/builds lo traen, así que hay que detectarlo con `mpv --vo=help`).
- Viabilidad: el modo embebido hoy usa `--vo=kitty` con `--vo-kitty-{left,top,width,height}`
  (player.rs ~257). `vo=sixel` acepta opciones análogas (`--vo-sixel-*`), por lo que
  integrarlo sería un cambio acotado: elegir `vo` según `Graphics`, y limpiar la zona al
  parar (Sixel no se borra, hay que repintar celdas, como ya se hace con las miniaturas).
- Limitaciones: la codificación Sixel es por CPU y a paleta (≤256 colores) → fps bajos
  (típicamente 5–15 según tamaño) y bandas de color; sin memoria compartida como kitty
  (`use-shm`); ratatui y mpv compiten por el mismo flujo de escape, y los terminales
  Sixel tienen tamaños de celda/pantalla distintos. Dentro de tmux/zellij no funciona
  igual que en kitty (hoy `Auto` ya excluye multiplexores).
- Recomendación: NO activar por defecto. Si se quiere, ofrecerlo como experimental y
  opt-in (`player.mode = "embedded"` + `TUBETUI_GRAPHICS=sixel`), detectando soporte de mpv
  con `--vo=help`, y probarlo primero en foot. Para el uso normal, fuera de kitty
  el modo ventana/overlay de mpv da mucha mejor calidad.

## Pendiente de revisión humana
- Probar las miniaturas en foot/mlterm reales.
- Decidir si se implementa el video Sixel experimental (no implementado).

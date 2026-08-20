# Prototype Instructions

Run the local server yourself and open the preview in the browser available to this environment. Do not give the user server-start instructions when you can run it.

Before making substantial visual changes, use the Product Design plugin's `get-context` skill when the visual source is unclear or no longer matches the current goal. When the user gives durable prototype-specific design feedback, preferences, or decisions, record them in `AGENTS.md`.

When implementing from a selected generated mock, treat that image as the source of truth for layout, component anatomy, density, spacing, color, typography, visible content, and hierarchy.

## Product decisions

- Use the selected dark edge-docked concept: a translucent category rail attached to the right edge with one floating expanded folder.
- Keep the interface minimal and allow the entire organizer to collapse into a narrow edge tab.
- Support adding custom category folders with a name and accent color.
- Closing the window should hide it to the system tray; the tray offers show, organize, pause/resume, and quit actions.
- The native window must be genuinely transparent and visually merge with the user's current desktop; do not render a simulated wallpaper or opaque app canvas.
- Only shortcuts successfully represented in a category may be moved off the desktop. Keep unclassified icons untouched and restore every managed shortcut to its original desktop when the tray app exits.
- Add a compact per-user Windows desktop context-menu submenu for showing, organizing, adding a category, and exiting; right-clicking an item inside a category should use that item's native Windows Shell context menu.

Build app UI in `src/`. Keep `.openai/hosting.json`, `worker/index.js`, `scripts/prepare-sites-build.mjs`, and `tests/sites-worker.test.mjs` intact so the same local prototype can be handed to Sites. Before a Sites handoff, run `npm run build` and `npm run test:sites`; the build must leave `dist/client/index.html`, `dist/server/index.js`, and `dist/.openai/hosting.json`.

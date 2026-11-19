# Design

## Restore on re-import

`ImportService::import` already looks up the hash before probing. When the match is trashed, call `assets::restore` and clear `missing`, then return the existing id with `restored: true` and `duplicate: false`. An active match keeps `duplicate: true`. `restored` is additive to the DTO, mirrored in TypeScript and the mock.

Restoring only flips `deleted_at`; cached thumbnails, proxies, and history references are untouched, which is why the asset reappears immediately.

## Remove in the Compose picker

The picker tiles are buttons, so the remove control becomes a sibling inside a relative wrapper, as in the Media grid. The handler confirms, calls `library_trash` with `assetIds`, reloads the lists, and lets the existing prune step drop the selection. No new IPC command.

## Mask canvas

The canvas currently fills black at load and paints white, which hides the image. Changes:

- Clear instead of filling; the mask layer is transparent by default.
- Render the asset thumbnail as an `<img>` behind the canvas in a relative stage, sized to the canvas aspect.
- Draw white for keep and use `destination-out` for the eraser so it clears coverage.
- Apply CSS opacity to the canvas only, so the image remains visible while mask data stays binary for export.

Export still reads the red channel, so a transparent pixel is 0 and a painted pixel is 255.

## Percentage crop

Replace the four pixel inputs with four range controls. Given the source `w × h` and edges `left/right/top/bottom` in percent (0–45 each, with left+right < 90 and top+bottom < 90):

```
x = round(w * left / 100)
width = clamp(round(w * (100 - left - right) / 100), 1, w - x)
```

The same formula runs for the vertical axis. Values are rounded and clamped, so the rectangle always satisfies the backend's `x + width <= w` and positive-size checks — the failure the user hit when typing partial pixel values.

Aspect presets keep using `cropToAspect`; moving any edge converts the result back into percentages, so the controls stay coherent. The preview overlay already renders from `crop`, so it updates live.

## Rejected alternatives

- **Creating a new record for the re-import**: duplicates cached files and splits history references.
- **Auto-deleting trash on re-import**: hides the restore model and loses the trash entry silently.
- **Drag-only crop rectangle**: better long term, but the edge controls are simpler, testable, and immediately solve the pixel-entry problem.

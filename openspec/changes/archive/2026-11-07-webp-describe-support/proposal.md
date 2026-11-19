# Proposal

## Why

WebP images import and thumbnail correctly but fail at the describe step, because the describer sends the original file bytes with `image/webp`. LM Studio's vision path accepts PNG/JPEG, so every WebP description errors before the model sees it.

## What Changes

- Still images whose format is not PNG or JPEG are transcribed once to a cached PNG with the bundled ffmpeg before being sent to the describer. This covers WebP, GIF, BMP, TIFF, and AVIF imports.
- The converted frame is keyed by the asset hash and reused on later describes.
- A conversion failure reports a clear media error instead of an opaque backend rejection.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `prompt-pipeline`: the describer normalizes unsupported still-image formats before sending them.

## Impact

- Backend: `commands/pipeline.rs` frame resolution and a transcoding helper; cached under `<cache>/describe/`.
- No new dependency (bundled ffmpeg already exists).
- No database migration.

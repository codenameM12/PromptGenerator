# Design

## Where

`resolve_asset_frames` in `commands/pipeline.rs` decides what is sent to the describer. For a still image it currently returns the source file verbatim, and `mime_for` then labels WebP as `image/webp`.

## Change

- For `asset.kind != "video"`, normalize the source:
  - PNG/JPEG → use as-is.
  - Anything else → run bundled ffmpeg with `-frames:v 1` into `<cache>/describe/<asset-hash>.png`; skip the run when the file already exists.
- Return the converted path; `mime_for` then reports `image/png`.
- Conversion failures surface as a media error that names the source path.
- Video describe and video edit paths are unchanged; sampled frames are already PNG/JPEG.

## Why ffmpeg instead of an image crate

The app already ships ffmpeg, which decodes WebP, AVIF, TIFF, BMP, and GIF (first frame). Adding an image-decoding dependency would increase build time and binary size for a single conversion.

## Cache

`<cache>/describe/` lives beside thumbnails and proxies, so clearing the cache removes conversions. The hash key means an edited or replaced source cannot collide.

## Risks

- Animated GIF/WebP describe only the first frame, matching a still-image description.
- Very large images are sent as decoded PNGs; LM Studio still enforces its own image limits.

# Design

## Latest edit data

One query per request is fine at this scale: add `edits::latest_for_asset`
(`ORDER BY created_at DESC LIMIT 1`) and iterate the requested asset ids in the
command (few assets, one small indexed `edits` table). `edits_latest` takes
`asset_ids: Vec<String>` and returns `Vec<LatestEdit>`; missing/unapplied
assets are simply absent from the result, which the renderer treats as
"source mode".

## Error context

`render.rs` and `mask.rs` both map `std::io` errors wholesale via
`CommandError::from_io`. Instead, when `create_dir_all` (or the final write)
fails with `PermissionDenied`, build a message:

```
Access denied while writing <folder>.
Choose a different output folder (Runtime settings → Output folder), or let
Windows trust this app (Windows Security → Ransomware protection) if the
folder is protected.
```

with details carrying the OS error text. Navigator fix is limited to the two
write sites for edited output and masks.

## Reference wiring

`ComposePanel` already keeps one `editsMap` fetched during `loadLists` via
`edits_latest`. Rules:

- Reference building (`runCompose`) and per-asset describe requests use
  `editsMap[assetId].editId ?? null`.
- Tile preview: `editsMap[assetId] ? mediaUrl(edit.outputPath) : mediaUrl(...)`
  thumbnail fallback chain, plus a static "edited" badge styled with an
  existing token-badge pattern.
- `App.tsx` bumps an `editsVersion` counter when `MediaEditor` reports a saved
  result (`onApplied` prop); `ComposePanel` refetches on `mediaVersion` or
  `editsVersion` change, so tiles update immediately after Apply without
  restarting the app.

Backend image resolution (`ComposerImages`) keeps edit-first resolution for
explicit `edit_id`, and falls back to `latest_for_asset` for asset-only
references — mirroring the picker exactly.

## Risks

- Old cached generations reference source content; unchanged, because the
  represent edit drives the edit cache key and compose cache key references.
- A removed/trashed asset's latest edit disappears with editing history; the
  map lookup just misses, restoring source-mode behavior.

## Open questions

None. The alternate output root already exists in settings; this change only
points users to it.

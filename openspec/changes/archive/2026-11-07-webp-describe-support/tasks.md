# Tasks

## 1. Describer format conversion

- [x] 1.1 Transcode non-PNG/JPEG still images to a cached PNG keyed by asset hash during describe frame resolution, reusing an existing conversion; verify a Rust test creates a WebP, converts it, asserts the PNG signature, and asserts a second call reuses the file
- [x] 1.2 Report a media error naming the source file when conversion fails; verify the error is not a raw LM Studio rejection

## 2. Verification

- [x] 2.1 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 2.2 Run `openspec validate webp-describe-support --strict`; verify it passes
- [x] 2.3 Rebuild after the user closes the app and describe a WebP image; verify the description succeeds and the source file is unchanged

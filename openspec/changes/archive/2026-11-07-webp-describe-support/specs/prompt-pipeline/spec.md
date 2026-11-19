# Spec Delta

## ADDED Requirements

### Requirement: Describer image format normalization

The system SHALL convert still images that are not PNG or JPEG into a cached PNG before sending them to the describer model, so formats such as WebP, GIF, BMP, TIFF, and AVIF can be described. Conversions SHALL be keyed by the asset hash, SHALL reuse an existing converted file, and SHALL report a clear error if conversion fails. Video frame sampling keeps using its existing JPEG/PNG output.

#### Scenario: Describe a WebP image

- **WHEN** the user describes an imported WebP image
- **THEN** a cached PNG version is produced and sent to the describer
- **AND** the description completes like a PNG or JPEG image
- **AND** the original WebP file is unchanged

#### Scenario: Reuse the converted file

- **WHEN** the same WebP asset is described again
- **THEN** the cached PNG is reused
- **AND** no second conversion is run

#### Scenario: Conversion failure

- **WHEN** the image cannot be decoded by the bundled ffmpeg
- **THEN** the describe fails with a media error naming the file
- **AND** the app does not send invalid bytes to LM Studio

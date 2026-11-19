# Spec Delta

## ADDED Requirements

### Requirement: Text-first preset guide

The preset editor SHALL offer a guide form that accepts a name, a kind, a target description, a prompt guide written as plain text, and an optional prompt-character limit. Saving the guide SHALL compile a schema-valid preset document by supplying defaults for fields the guide does not cover, SHALL run the same validation as JSON authoring, and SHALL create or update a user preset. The raw JSON editor SHALL remain available and SHALL operate on the same stored document, and built-in presets SHALL stay read-only in both views.

#### Scenario: Create a preset from a text guide

- **WHEN** the user enters a name, chooses a kind, writes a prompt guide, and saves
- **THEN** a user preset is created with its own identity and version
- **AND** it appears in the target selector
- **AND** its compiled instructions contain the guide text

#### Scenario: Guide and JSON stay consistent

- **WHEN** the user saves a guide and then opens the JSON view
- **THEN** the JSON shows the compiled document that was validated and stored
- **AND** editing that JSON and saving updates the stored preset

#### Scenario: Incomplete guide is rejected

- **WHEN** the user saves a guide with an empty name or empty guide text
- **THEN** the save is rejected with a specific message
- **AND** no preset is created or overwritten

#### Scenario: Built-in preset is protected

- **WHEN** the user opens a built-in preset
- **THEN** the guide fields are read-only
- **AND** duplicating it produces an editable user preset

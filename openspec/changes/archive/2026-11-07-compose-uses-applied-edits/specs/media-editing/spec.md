# Spec Delta

## ADDED Requirements

### Requirement: Actionable permission failures

The system SHALL report write-failures for edited output and exported masks
with the blocked folder named and the way out explained: choosing another
output folder, or allowing the app in Windows ransomware protection when the
folder is protected by the system.

#### Scenario: Protected pictures folder

- **WHEN** applying an edit fails because the source folder denies writes
- **THEN** the message names the source folder
- **AND** the message points to the output-folder setting or the Windows
  ransomware-protection exception
- **AND** "Access is denied (os error 5)" never appears without context

### Requirement: Configurable edit output folder

The Runtime view SHALL expose an **Edit output** folder with Browse, Save, and
**Use source folder** actions, persisting an absolute alternate root for
applied edits and mask exports, so a Windows-protected source folder (such as
Pictures) cannot block editing with no remedy.

#### Scenario: Move edit output to a writable folder

- **WHEN** the user picks a writable folder and saves it
- **THEN** applying a crop or mask writes under `<folder>\edited\`
- **AND** the source file is unchanged
- **AND** **Use source folder** restores the beside-the-source default

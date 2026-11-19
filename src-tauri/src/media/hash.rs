use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::commands::CommandError;

/// Content hash used for deduplication and cache keys.
pub fn hash_file(path: &Path) -> Result<String, CommandError> {
    let mut file = File::open(path).map_err(|error| CommandError::from_io(&error))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| CommandError::from_io(&error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::hash_file;

    #[test]
    fn hashes_file_contents() {
        let unique = format!(
            "prompt-studio-hash-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::write(&path, b"hello").expect("write");
        let expected = blake3::hash(b"hello").to_hex().to_string();
        assert_eq!(hash_file(&path).expect("hash"), expected);
        let _ = std::fs::remove_file(path);
    }
}

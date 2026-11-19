use anyhow::Result;
use std::{fs::File, io::Write, path::Path};

/// Publish only a complete file. A failed encode, flush or replacement preserves the destination.
pub fn replace(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let mut tmp = tempfile::Builder::new().prefix(".ocrtrans-write-").tempfile_in(path.parent().unwrap_or(Path::new(".")))?;
    write(tmp.as_file_mut())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

pub fn bytes(path: &Path, data: &[u8]) -> Result<()> {
    replace(path, |f| Ok(f.write_all(data)?))
}

#[test]
fn failed_overwrite_preserves_previous_contents_and_removes_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("export.png");
    bytes(&dest, b"original").unwrap();
    assert!(replace(&dest, |f| { f.write_all(b"partial")?; anyhow::bail!("disk full") }).is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"original");
    assert_eq!(dir.path().read_dir().unwrap().count(), 1);
    bytes(&dest, b"new").unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), b"new");
}

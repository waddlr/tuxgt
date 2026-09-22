use std::path::Path;

pub(crate) fn write_file(p: &Path, b: &[u8]) {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(p, b).unwrap();
}

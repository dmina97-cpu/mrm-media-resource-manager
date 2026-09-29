//! Đọc bên trong file nén ZIP / 7Z / RAR (chỉ đọc). Giải nén ĐÚNG một file cần xem vào cache của app —
//! file nén gốc không bao giờ bị sửa.
use std::io::{Read, Write};
use std::path::Path;

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Zip,
    SevenZ,
    Rar,
}

/// Loại file nén theo đuôi (xem được bên trong). `.part1.rar`, `.r00`… vẫn là RAR.
pub fn kind_of(path: &Path) -> Option<Kind> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.ends_with(".zip") {
        Some(Kind::Zip)
    } else if name.ends_with(".7z") {
        Some(Kind::SevenZ)
    } else if name.ends_with(".rar") {
        Some(Kind::Rar)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// đường dẫn bên trong, dùng '/'
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

fn norm(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches('/').to_string()
}

/// Liệt kê mọi mục trong file nén.
pub fn list(path: &Path) -> Res<Vec<Entry>> {
    match kind_of(path).ok_or("Định dạng nén chưa hỗ trợ")? {
        Kind::Zip => {
            let f = std::fs::File::open(path).map_err(e)?;
            let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(e)?;
            let mut out = Vec::with_capacity(z.len());
            for i in 0..z.len() {
                let Ok(f) = z.by_index_raw(i) else { continue };
                out.push(Entry { name: norm(f.name()), size: f.size(), is_dir: f.is_dir() });
            }
            Ok(out)
        }
        Kind::SevenZ => {
            let r = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty()).map_err(|err| friendly(err.to_string()))?;
            Ok(r.archive().files.iter().map(|f| Entry { name: norm(f.name()), size: f.size(), is_dir: f.is_directory() }).collect())
        }
        Kind::Rar => {
            let mut out = Vec::new();
            let archive = unrar::Archive::new(path).open_for_listing().map_err(|err| friendly(err.to_string()))?;
            for h in archive {
                let h = h.map_err(|err| friendly(err.to_string()))?;
                out.push(Entry { name: norm(&h.filename.to_string_lossy()), size: h.unpacked_size, is_dir: h.is_directory() });
            }
            Ok(out)
        }
    }
}

fn friendly(msg: String) -> String {
    if std::env::var("MRM_ARCHIVE_DEBUG").is_ok() {
        eprintln!("archive raw error: {msg}");
    }
    let l = msg.to_lowercase();
    if l.contains("password") || l.contains("encrypt") {
        "File nén có mật khẩu — không xem được bên trong".into()
    } else {
        format!("Không đọc được file nén: {msg}")
    }
}

/// Giải nén một mục vào `out` (ghi file tạm rồi đổi tên — không để lại file dở).
pub fn extract(path: &Path, entry: &str, out: &Path, limit: u64) -> Res<()> {
    let want = norm(entry);
    let tmp = out.with_extension("part");
    let result = (|| -> Res<()> {
        match kind_of(path).ok_or("Định dạng nén chưa hỗ trợ")? {
            Kind::Zip => {
                let f = std::fs::File::open(path).map_err(e)?;
                let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(e)?;
                let mut f = z.by_name(&want).map_err(e)?;
                if f.size() > limit {
                    return Err("File trong file nén quá lớn để xem trước".into());
                }
                let mut w = std::fs::File::create(&tmp).map_err(e)?;
                std::io::copy(&mut (&mut f).take(limit + 1), &mut w).map_err(e)?;
                w.flush().map_err(e)?;
            }
            Kind::SevenZ => {
                let mut r = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty()).map_err(|err| friendly(err.to_string()))?;
                let (name, size) = r
                    .archive()
                    .files
                    .iter()
                    .find(|f| norm(f.name()) == want)
                    .map(|f| (f.name().to_string(), f.size()))
                    .ok_or("Không thấy file trong file nén")?;
                if size > limit {
                    return Err("File trong file nén quá lớn để xem trước".into());
                }
                let data = r.read_file(&name).map_err(|err| friendly(err.to_string()))?;
                std::fs::write(&tmp, data).map_err(e)?;
            }
            Kind::Rar => {
                let mut a = unrar::Archive::new(path).open_for_processing().map_err(|err| friendly(err.to_string()))?;
                loop {
                    let Some(h) = a.read_header().map_err(|err| friendly(err.to_string()))? else {
                        return Err("Không thấy file trong file nén".into());
                    };
                    if norm(&h.entry().filename.to_string_lossy()) == want {
                        if h.entry().unpacked_size > limit {
                            return Err("File trong file nén quá lớn để xem trước".into());
                        }
                        h.extract_to(&tmp).map_err(|err| friendly(err.to_string()))?;
                        break;
                    }
                    a = h.skip().map_err(|err| friendly(err.to_string()))?;
                }
            }
        }
        Ok(())
    })();
    if let Err(err) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    std::fs::rename(&tmp, out).map_err(e)
}

/// Đọc tối đa `limit` byte đầu của một mục (xem text). Trả về (dữ liệu, kích thước thật).
pub fn read_head(path: &Path, entry: &str, limit: usize, cache_dir: &Path) -> Res<(Vec<u8>, u64)> {
    let size = list(path)?.into_iter().find(|x| x.name == norm(entry)).map(|x| x.size).ok_or("Không thấy file trong file nén")?;
    std::fs::create_dir_all(cache_dir).map_err(e)?;
    let out = cache_dir.join(format!("text-{}.bin", std::process::id()));
    extract(path, entry, &out, 64 << 20)?;
    let mut buf = Vec::new();
    std::fs::File::open(&out).map_err(e)?.take(limit as u64).read_to_end(&mut buf).map_err(e)?;
    let _ = std::fs::remove_file(&out);
    Ok((buf, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrm_archive_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn zip_list_and_extract() {
        let d = tmp("zip");
        let zp = d.join("pack.zip");
        {
            let f = std::fs::File::create(&zp).unwrap();
            let mut z = zip::ZipWriter::new(f);
            z.add_directory("Preview/", zip::write::SimpleFileOptions::default()).unwrap();
            z.start_file("Preview/cover.txt", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"hello cover").unwrap();
            z.finish().unwrap();
        }
        let l = list(&zp).unwrap();
        assert!(l.iter().any(|x| x.name == "Preview/cover.txt" && x.size == 11 && !x.is_dir));
        let out = d.join("x.txt");
        extract(&zp, "Preview/cover.txt", &out, 1 << 20).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"hello cover");
        assert!(extract(&zp, "Preview/cover.txt", &d.join("y.txt"), 5).unwrap_err().contains("quá lớn"));
        assert!(!d.join("y.part").exists(), "không để lại file dở");
        assert_eq!(kind_of(Path::new("a.PART1.RAR")), Some(Kind::Rar));
        assert_eq!(kind_of(Path::new("a.7z")), Some(Kind::SevenZ));
        assert_eq!(kind_of(Path::new("a.tar.gz")), None);
    }

    /// Thử trên file 7z/rar thật: MRM_ARCHIVE_TEST=<file.7z|file.rar> cargo test real_archive -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_archive() {
        let Ok(p) = std::env::var("MRM_ARCHIVE_TEST") else { return };
        let p = std::path::PathBuf::from(p);
        let l = list(&p).unwrap();
        println!("{} mục", l.len());
        let first = l.iter().find(|x| !x.is_dir && x.size > 0 && x.size < 50 << 20).expect("có file");
        let out = tmp("real").join("out.bin");
        extract(&p, &first.name, &out, 64 << 20).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().len(), first.size);
        println!("giải nén {} ({} byte) OK", first.name, first.size);
    }
}

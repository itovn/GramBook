//! Chuyển một sách Kindle rồi ghi ra thư mục để xem bằng mắt (không dùng trong app):
//! `cargo run -p gb-mobi --release --example dump -- <tệp sách> <thư mục ra>`.
//! Ghi các tệp chương / tài nguyên theo đường dẫn trong sách, kèm `info.txt` (thông tin + mục lục).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("dùng: dump <tệp sách> <thư mục ra>");
        std::process::exit(2);
    }
    let data = match fs::read(&args[1]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("không đọc được {}: {e}", args[1]);
            std::process::exit(1);
        }
    };
    let t = std::time::Instant::now();
    let book = match gb_mobi::convert(&data) {
        Ok(b) => b,
        Err(e) => {
            println!("lỗi: {e} ({e:?})");
            std::process::exit(1);
        }
    };
    let took = t.elapsed();
    let out = Path::new(&args[2]);
    for it in book.spine.iter().chain(&book.resources) {
        let p = out.join(&it.path);
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Err(e) = fs::write(&p, &it.data) {
            eprintln!("không ghi được {}: {e}", p.display());
        }
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "định dạng: {:?}\ntên: {}\ntác giả: {:?}",
        book.format, book.title, book.authors
    );
    let _ = writeln!(
        s,
        "ngôn ngữ: {:?}\nnhà xuất bản: {:?}\nbìa: {:?}",
        book.language, book.publisher, book.cover
    );
    let _ = writeln!(s, "giới thiệu: {:?}", book.description);
    let _ = writeln!(
        s,
        "chương: {}  tài nguyên: {}  thời gian: {took:?}\n",
        book.spine.len(),
        book.resources.len()
    );
    for it in &book.spine {
        let _ = writeln!(s, "  {} {} {} B", it.path, it.mime, it.data.len());
    }
    for it in &book.resources {
        let _ = writeln!(s, "  {} {} {} B", it.path, it.mime, it.data.len());
    }
    let _ = writeln!(s, "\nmục lục ({}):", book.toc.len());
    for e in &book.toc {
        let _ = writeln!(
            s,
            "{}{}  →  {}",
            "  ".repeat(e.depth as usize),
            e.title,
            e.href
        );
    }
    let _ = fs::write(out.join("info.txt"), &s);
    println!(
        "{:?} «{}» — {} chương, {} tài nguyên, {} mục lục, {took:?} → {}",
        book.format,
        book.title,
        book.spine.len(),
        book.resources.len(),
        book.toc.len(),
        out.display()
    );
}

//! Việc đọc của từng cuốn (`reading.json`): vị trí đang đọc (CFI rút gọn — THIET-KE 5.2), dấu trang, tiến độ, đã đọc xong, thời gian
//! đọc. Không bao giờ bị dọn theo bộ nhớ đệm (sách bị xoá khỏi máy thì mở lại vẫn về đúng chỗ). Mục «Đang đọc» của app lấy ở đây.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::library::write_atomic;

/// Một vị trí trong sách.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Pos {
    /// Chương (chỉ số spine).
    pub s: u32,
    /// Phần của chương (chương lớn bị cắt — chapter.rs).
    #[serde(default)]
    pub p: u32,
    /// «/3/1/5:120» — đường dẫn childNodes từ <body> + vị trí ký tự (reader.js `GB.pos`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<String>,
    /// Tỉ lệ trong phần (dự phòng khi đường dẫn hỏng).
    #[serde(default)]
    pub f: f32,
    /// Vài chữ ở vị trí đó (hiện ở danh sách dấu trang; tìm lại khi đường dẫn hỏng).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub t: String,
}

/// Dấu trang.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Mark {
    pub id: u32,
    pub pos: Pos,
    pub date: i64,
    /// Tên chương lúc đặt dấu.
    #[serde(default)]
    pub chapter: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Reading {
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub ext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<Pos>,
    /// Tiến độ cả sách 0…1 (app ước: chương + phần + trang).
    #[serde(default)]
    pub progress: f32,
    /// Tên chương đang đọc (hiện ở «Đọc tiếp»).
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub started: i64,
    /// Lần mở gần nhất (Unix giây).
    #[serde(default)]
    pub opened: i64,
    #[serde(default)]
    pub finished: bool,
    /// Tổng thời gian đọc (giây).
    #[serde(default)]
    pub seconds: u64,
    #[serde(default)]
    pub marks: Vec<Mark>,
    #[serde(default)]
    next_mark: u32,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub struct Readings {
    path: PathBuf,
    map: HashMap<String, Reading>,
    dirty: bool,
    last_write: Option<Instant>,
}

impl Readings {
    pub fn open(data_dir: &Path) -> Readings {
        let path = data_dir.join("reading.json");
        let map: HashMap<String, Reading> = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Reading>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.key.clone(), r))
            .collect();
        Readings { path, map, dirty: false, last_write: None }
    }

    pub fn get(&self, key: &str) -> Option<&Reading> {
        self.map.get(key)
    }

    /// Ghi xuống đĩa nếu có đổi.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let mut v: Vec<&Reading> = self.map.values().collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.opened));
        write_atomic(&self.path, &serde_json::to_vec(&v).unwrap_or_default())?;
        self.dirty = false;
        self.last_write = Some(Instant::now());
        Ok(())
    }

    /// Ghi ngay nếu lần ghi trước đã lâu (vị trí đổi mỗi lần lật trang — gộp, ≤ 1 lần / 5 giây).
    fn soon(&mut self) {
        self.dirty = true;
        if self.last_write.is_none_or(|t| t.elapsed().as_secs() >= 5) {
            let _ = self.flush();
        }
    }

    /// Mở sách: tạo / cập nhật bản ghi, ghi lần mở.
    pub fn opened(&mut self, key: &str, title: &str, author: &str, ext: &str) -> &Reading {
        let t = now();
        let r = self.map.entry(key.to_string()).or_insert_with(|| Reading { key: key.to_string(), started: t, ..Default::default() });
        if !title.is_empty() {
            r.title = title.to_string();
        }
        if !author.is_empty() {
            r.author = author.to_string();
        }
        if !ext.is_empty() {
            r.ext = ext.to_string();
        }
        r.opened = t;
        self.dirty = true;
        let _ = self.flush();
        &self.map[key]
    }

    /// Lưu vị trí đang đọc (+ tiến độ, chương, thời gian đọc thêm).
    pub fn save_pos(&mut self, key: &str, pos: Pos, progress: Option<f32>, chapter: Option<&str>, add_seconds: u64) {
        let Some(r) = self.map.get_mut(key) else { return };
        r.pos = Some(pos);
        if let Some(p) = progress {
            r.progress = p.clamp(0.0, 1.0);
            if r.progress >= 0.995 {
                r.finished = true;
            }
        }
        if let Some(c) = chapter {
            r.chapter = c.to_string();
        }
        r.seconds += add_seconds.min(3600);
        self.soon();
    }

    pub fn set_finished(&mut self, key: &str, on: bool) {
        if let Some(r) = self.map.get_mut(key) {
            r.finished = on;
            self.dirty = true;
            let _ = self.flush();
        }
    }

    /// Thêm dấu trang (cùng vị trí thì thôi). Trả id.
    pub fn add_mark(&mut self, key: &str, pos: Pos, chapter: &str) -> Option<u32> {
        let r = self.map.get_mut(key)?;
        if let Some(m) = r.marks.iter().find(|m| m.pos.s == pos.s && m.pos.p == pos.p && m.pos.c == pos.c) {
            return Some(m.id);
        }
        r.next_mark += 1;
        let id = r.next_mark;
        r.marks.push(Mark { id, pos, date: now(), chapter: chapter.to_string() });
        r.marks.sort_by_key(|m| (m.pos.s, m.pos.p, m.pos.f.to_bits()));
        self.dirty = true;
        let _ = self.flush();
        Some(id)
    }

    pub fn remove_mark(&mut self, key: &str, id: u32) -> bool {
        let Some(r) = self.map.get_mut(key) else { return false };
        let n = r.marks.len();
        r.marks.retain(|m| m.id != id);
        let changed = r.marks.len() != n;
        if changed {
            self.dirty = true;
            let _ = self.flush();
        }
        changed
    }

    /// Xoá việc đọc của một cuốn (bắt đầu lại).
    pub fn forget(&mut self, key: &str) {
        if self.map.remove(key).is_some() {
            self.dirty = true;
            let _ = self.flush();
        }
    }

    /// Sách mở gần đây (mới → cũ). `finished`: Some(true) chỉ sách đã đọc xong, Some(false) chỉ sách đang đọc dở.
    pub fn recent(&self, limit: usize, finished: Option<bool>) -> Vec<&Reading> {
        let mut v: Vec<&Reading> = self.map.values().filter(|r| finished.is_none_or(|f| r.finished == f)).collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.opened));
        v.truncate(limit);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_marks_recent() {
        let d = std::env::temp_dir().join(format!("gb-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut r = Readings::open(&d);
        r.opened("1_1", "Ai Cập Huyền Bí", "Paul Brunton", "epub");
        let p = Pos { s: 3, p: 1, c: Some("/2/1:40".into()), f: 0.4, t: "Người du khách".into() };
        r.save_pos("1_1", p.clone(), Some(0.3), Some("Chương 2"), 120);
        let id = r.add_mark("1_1", p.clone(), "Chương 2").unwrap();
        assert_eq!(r.add_mark("1_1", p.clone(), "Chương 2"), Some(id));
        r.opened("2_2", "Bê Trọc", "", "epub");
        r.save_pos("2_2", Pos::default(), Some(1.0), None, 0);
        r.flush().unwrap();
        let again = Readings::open(&d);
        let a = again.get("1_1").unwrap();
        assert_eq!((a.pos.as_ref(), a.progress, a.seconds, a.marks.len()), (Some(&p), 0.3, 120, 1));
        assert!(again.get("2_2").unwrap().finished);
        assert_eq!(again.recent(10, Some(false)).iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), vec!["1_1"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}

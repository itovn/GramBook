//! Bộ sưu tập (`collections.json`): bộ tự tạo, Yêu thích, Muốn đọc, Mở gần đây. Mỗi mục giữ khoá sách + bản chép tên / tác giả,
//! nên vẫn hiện được khi nguồn chứa sách đã bị gỡ khỏi thư viện. Chép từ playlist của GramMusic (giữ tên tệp / lớp để so khác
//! biệt dễ). «Đã đọc xong» lấy từ vị trí đọc (reading.rs), không lưu ở đây.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::{write_atomic, Book};

/// Giữ tối đa chừng này sách «Mở gần đây».
const HISTORY_MAX: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub ext: String,
}

impl From<&Book> for Item {
    fn from(b: &Book) -> Item {
        Item { key: b.key(), title: b.shown_title().to_string(), author: b.shown_author().to_string(), ext: b.ext.clone() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Playlist {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct Data {
    #[serde(default)]
    next_id: u32,
    #[serde(default)]
    lists: Vec<Playlist>,
    #[serde(default)]
    fav: Vec<Item>,
    /// «Muốn đọc».
    #[serde(default)]
    want: Vec<Item>,
    #[serde(default)]
    history: Vec<Item>,
}

pub struct Playlists {
    path: PathBuf,
    d: Data,
    favs: HashSet<String>,
    wants: HashSet<String>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Playlists {
    pub fn open(data_dir: &Path) -> Playlists {
        let path = data_dir.join("collections.json");
        let d: Data = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let favs = d.fav.iter().map(|i| i.key.clone()).collect();
        let wants = d.want.iter().map(|i| i.key.clone()).collect();
        Playlists { path, d, favs, wants }
    }

    fn save(&self) -> std::io::Result<()> {
        write_atomic(&self.path, &serde_json::to_vec(&self.d).unwrap_or_default())
    }

    pub fn lists(&self) -> &[Playlist] {
        &self.d.lists
    }

    pub fn get(&self, id: u32) -> Option<&Playlist> {
        self.d.lists.iter().find(|p| p.id == id)
    }

    pub fn create(&mut self, name: &str) -> std::io::Result<u32> {
        self.d.next_id = self.d.next_id.max(1);
        let id = self.d.next_id;
        self.d.next_id += 1;
        let t = now();
        self.d.lists.push(Playlist { id, name: name.trim().to_string(), items: Vec::new(), created: t, updated: t });
        self.save()?;
        Ok(id)
    }

    fn with(&mut self, id: u32, f: impl FnOnce(&mut Playlist)) -> std::io::Result<bool> {
        let Some(p) = self.d.lists.iter_mut().find(|p| p.id == id) else { return Ok(false) };
        f(p);
        p.updated = now();
        self.save()?;
        Ok(true)
    }

    pub fn rename(&mut self, id: u32, name: &str) -> std::io::Result<bool> {
        let name = name.trim().to_string();
        self.with(id, |p| p.name = name)
    }

    pub fn delete(&mut self, id: u32) -> std::io::Result<bool> {
        let before = self.d.lists.len();
        self.d.lists.retain(|p| p.id != id);
        self.save()?;
        Ok(before != self.d.lists.len())
    }

    /// Thêm sách vào cuối (sách đã có trong bộ thì bỏ qua). Trả số sách đã thêm.
    pub fn add(&mut self, id: u32, items: Vec<Item>) -> std::io::Result<usize> {
        let mut n = 0;
        self.with(id, |p| {
            let mut have: HashSet<String> = p.items.iter().map(|i| i.key.clone()).collect();
            for it in items {
                if have.insert(it.key.clone()) {
                    p.items.push(it);
                    n += 1;
                }
            }
        })?;
        Ok(n)
    }

    pub fn remove(&mut self, id: u32, keys: &[String]) -> std::io::Result<bool> {
        let ks: HashSet<&String> = keys.iter().collect();
        self.with(id, |p| p.items.retain(|i| !ks.contains(&i.key)))
    }

    /// Đổi chỗ một sách (kéo thả trong chế độ Sửa).
    pub fn move_item(&mut self, id: u32, from: usize, to: usize) -> std::io::Result<bool> {
        self.with(id, |p| {
            if from < p.items.len() {
                let it = p.items.remove(from);
                p.items.insert(to.min(p.items.len()), it);
            }
        })
    }

    pub fn is_fav(&self, key: &str) -> bool {
        self.favs.contains(key)
    }

    pub fn favs(&self) -> &[Item] {
        &self.d.fav
    }

    /// Bật / tắt Yêu thích; trả trạng thái mới. Sách mới thích lên đầu.
    pub fn set_fav(&mut self, item: Item, on: bool) -> std::io::Result<bool> {
        self.d.fav.retain(|i| i.key != item.key);
        self.favs.remove(&item.key);
        if on {
            self.favs.insert(item.key.clone());
            self.d.fav.insert(0, item);
        }
        self.save()?;
        Ok(on)
    }

    pub fn is_want(&self, key: &str) -> bool {
        self.wants.contains(key)
    }

    pub fn wants(&self) -> &[Item] {
        &self.d.want
    }

    /// Bật / tắt «Muốn đọc»; trả trạng thái mới. Sách mới thêm lên đầu.
    pub fn set_want(&mut self, item: Item, on: bool) -> std::io::Result<bool> {
        self.d.want.retain(|i| i.key != item.key);
        self.wants.remove(&item.key);
        if on {
            self.wants.insert(item.key.clone());
            self.d.want.insert(0, item);
        }
        self.save()?;
        Ok(on)
    }

    pub fn history(&self) -> &[Item] {
        &self.d.history
    }

    /// Ghi sách vừa mở lên đầu «Mở gần đây».
    pub fn played(&mut self, item: Item) -> std::io::Result<()> {
        self.d.history.retain(|i| i.key != item.key);
        self.d.history.insert(0, item);
        self.d.history.truncate(HISTORY_MAX);
        self.save()
    }

    pub fn clear_history(&mut self) -> std::io::Result<()> {
        self.d.history.clear();
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(k: &str) -> Item {
        Item { key: k.into(), title: format!("T{k}"), author: String::new(), ext: "epub".into() }
    }

    #[test]
    fn playlists_roundtrip() {
        let d = std::env::temp_dir().join(format!("gb-pl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut p = Playlists::open(&d);
        let a = p.create(" Sách hay ").unwrap();
        let b = p.create("Đọc dần").unwrap();
        assert_ne!(a, b);
        assert_eq!(p.add(a, vec![it("1_1"), it("1_2"), it("1_1")]).unwrap(), 2);
        assert_eq!(p.add(a, vec![it("1_2"), it("1_3")]).unwrap(), 1);
        p.move_item(a, 2, 0).unwrap();
        p.remove(a, &["1_2".into()]).unwrap();
        p.set_fav(it("1_9"), true).unwrap();
        p.set_want(it("1_8"), true).unwrap();
        p.set_want(it("1_7"), true).unwrap();
        p.set_want(it("1_8"), false).unwrap();
        p.played(it("1_1")).unwrap();
        p.played(it("1_3")).unwrap();
        p.played(it("1_1")).unwrap();
        p.delete(b).unwrap();
        let q = Playlists::open(&d);
        assert_eq!(q.lists().len(), 1);
        assert_eq!(q.get(a).unwrap().name, "Sách hay");
        assert_eq!(q.get(a).unwrap().items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["1_3", "1_1"]);
        assert!(q.is_fav("1_9") && !q.is_fav("1_1"));
        assert!(q.is_want("1_7") && !q.is_want("1_8"));
        assert_eq!(q.history().iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["1_1", "1_3"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}

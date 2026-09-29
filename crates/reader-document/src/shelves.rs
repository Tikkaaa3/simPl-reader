//! User-made shelves such as "To read", "Finished" or "Course".
//!
//! Shelves live in their own file so library.json keeps its schema and older
//! versions still open it. A book belongs to a shelf by content fingerprint,
//! so relocating the file keeps its shelves.
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::position;

const VERSION: u32 = 1;
pub const MAX_SHELVES: usize = 64;
pub const MAX_NAME_CHARS: usize = 40;
const MAX_BOOKS: usize = 4096;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shelf {
    pub id: u64,
    pub name: String,
    /// SHA-256 fingerprints of the documents on this shelf.
    pub books: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shelves {
    version: u32,
    next_id: u64,
    pub shelves: Vec<Shelf>,
}

impl Shelves {
    pub fn get(&self, id: u64) -> Option<&Shelf> {
        self.shelves.iter().find(|shelf| shelf.id == id)
    }

    /// Adds a shelf at the end and returns its id.
    pub fn create(&mut self, name: &str) -> Result<u64, String> {
        if self.shelves.len() >= MAX_SHELVES {
            return Err(format!("You can have at most {MAX_SHELVES} shelves."));
        }
        let name = self.checked_name(name, None)?;
        self.next_id = self
            .next_id
            .max(self.shelves.iter().map(|s| s.id).max().unwrap_or(0))
            + 1;
        self.shelves.push(Shelf {
            id: self.next_id,
            name,
            books: Vec::new(),
        });
        Ok(self.next_id)
    }

    pub fn rename(&mut self, id: u64, name: &str) -> Result<(), String> {
        let name = self.checked_name(name, Some(id))?;
        let shelf = self
            .shelves
            .iter_mut()
            .find(|shelf| shelf.id == id)
            .ok_or("That shelf no longer exists.")?;
        shelf.name = name;
        Ok(())
    }

    /// Removes the shelf only; its books stay in the library.
    pub fn delete(&mut self, id: u64) {
        self.shelves.retain(|shelf| shelf.id != id);
    }

    pub fn contains(&self, id: u64, fingerprint: &str) -> bool {
        self.get(id)
            .is_some_and(|shelf| shelf.books.iter().any(|book| book == fingerprint))
    }

    /// Puts the book on the shelf or takes it off; returns whether it is on it now.
    pub fn toggle(&mut self, id: u64, fingerprint: &str) -> Result<bool, String> {
        let shelf = self
            .shelves
            .iter_mut()
            .find(|shelf| shelf.id == id)
            .ok_or("That shelf no longer exists.")?;
        if let Some(position) = shelf.books.iter().position(|book| book == fingerprint) {
            shelf.books.remove(position);
            return Ok(false);
        }
        if shelf.books.len() >= MAX_BOOKS {
            return Err(format!("A shelf holds at most {MAX_BOOKS} books."));
        }
        shelf.books.push(fingerprint.to_owned());
        Ok(true)
    }

    /// The shelves holding this book, in shelf order.
    pub fn of<'a>(&'a self, fingerprint: &'a str) -> impl Iterator<Item = &'a Shelf> + 'a {
        self.shelves
            .iter()
            .filter(move |shelf| shelf.books.iter().any(|book| book == fingerprint))
    }

    /// Takes a book that left the library off every shelf.
    pub fn forget(&mut self, fingerprint: &str) -> bool {
        let mut changed = false;
        for shelf in &mut self.shelves {
            let before = shelf.books.len();
            shelf.books.retain(|book| book != fingerprint);
            changed |= shelf.books.len() != before;
        }
        changed
    }

    fn checked_name(&self, name: &str, renaming: Option<u64>) -> Result<String, String> {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return Err("A shelf needs a name.".into());
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(format!(
                "Shelf names are at most {MAX_NAME_CHARS} characters."
            ));
        }
        let key = name.to_lowercase();
        if self
            .shelves
            .iter()
            .any(|shelf| Some(shelf.id) != renaming && shelf.name.to_lowercase() == key)
        {
            return Err(format!(
                "There is already a shelf named \u{201c}{name}\u{201d}."
            ));
        }
        Ok(name)
    }
}

fn validate(shelves: &Shelves) -> Result<(), String> {
    if shelves.version != VERSION {
        return Err(format!("unsupported shelves version {}", shelves.version));
    }
    if shelves.shelves.len() > MAX_SHELVES {
        return Err(format!("more than {MAX_SHELVES} shelves"));
    }
    let mut ids = Vec::new();
    for shelf in &shelves.shelves {
        let name = shelf.name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || shelf.books.len() > MAX_BOOKS
        {
            return Err("a shelf has an invalid name or too many books".into());
        }
        if shelf
            .books
            .iter()
            .any(|book| book.len() != 64 || !book.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("a shelf lists an invalid book fingerprint".into());
        }
        ids.push(shelf.id);
    }
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("two shelves share an id".into());
    }
    Ok(())
}

fn shelves_path() -> PathBuf {
    position::storage_base().join("simPl").join("shelves.json")
}

/// A missing file means no shelves yet.
pub fn load() -> Result<Shelves, String> {
    load_at(&shelves_path())
}

fn load_at(path: &Path) -> Result<Shelves, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Shelves {
                version: VERSION,
                ..Shelves::default()
            });
        }
        Err(error) => return Err(format!("cannot open shelves {}: {error}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read shelves {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("shelves exceed size limit".into());
    }
    let shelves: Shelves = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid shelves {}: {error}", path.display()))?;
    validate(&shelves)?;
    Ok(shelves)
}

/// Atomically replace the shelves file.
pub fn save(shelves: &Shelves) -> Result<(), String> {
    save_at(&shelves_path(), shelves)
}

fn save_at(path: &Path, shelves: &Shelves) -> Result<(), String> {
    let shelves = Shelves {
        version: VERSION,
        ..shelves.clone()
    };
    validate(&shelves)?;
    let bytes =
        serde_json::to_vec(&shelves).map_err(|error| format!("cannot encode shelves: {error}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("shelves exceed size limit".into());
    }
    position::atomic_write(path, &bytes, "shelves", "shelves", ".shelves")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(n: u8) -> String {
        format!("{n:064x}")
    }

    fn empty() -> Shelves {
        Shelves {
            version: VERSION,
            ..Shelves::default()
        }
    }

    #[test]
    fn shelves_are_created_renamed_and_deleted_with_unique_names() {
        let mut shelves = empty();
        let to_read = shelves.create("  To   read ").unwrap();
        assert_eq!(shelves.get(to_read).unwrap().name, "To read");
        assert!(shelves.create("to READ").is_err());
        assert!(shelves.create("   ").is_err());
        assert!(shelves.create(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        let course = shelves.create("Course").unwrap();
        assert_ne!(to_read, course);
        assert!(shelves.rename(course, "to read").is_err());
        shelves.rename(course, "Course 101").unwrap();
        shelves.rename(course, "course 101").unwrap();
        shelves.delete(to_read);
        assert!(shelves.get(to_read).is_none());
        // Ids are never reused, even after a delete.
        let next = shelves.create("Finished").unwrap();
        assert!(next > course);
    }

    #[test]
    fn books_move_on_and_off_shelves_and_leave_with_the_library() {
        let mut shelves = empty();
        let a = shelves.create("A").unwrap();
        let b = shelves.create("B").unwrap();
        assert!(shelves.toggle(a, &book(1)).unwrap());
        assert!(shelves.toggle(b, &book(1)).unwrap());
        assert!(shelves.toggle(a, &book(2)).unwrap());
        assert_eq!(shelves.of(&book(1)).count(), 2);
        assert!(!shelves.toggle(b, &book(1)).unwrap());
        assert!(!shelves.contains(b, &book(1)));
        assert!(shelves.forget(&book(1)));
        assert!(!shelves.forget(&book(1)));
        assert_eq!(shelves.get(a).unwrap().books, vec![book(2)]);
        assert!(shelves.toggle(99, &book(1)).is_err());
    }

    #[test]
    fn shelves_round_trip_and_reject_invalid_files() {
        let folder =
            std::env::temp_dir().join(format!("simpl-shelves-test-{}", std::process::id()));
        let path = folder.join("shelves.json");
        let _ = std::fs::remove_file(&path);
        assert!(load_at(&path).unwrap().shelves.is_empty());
        let mut shelves = empty();
        let id = shelves.create("Ders").unwrap();
        shelves.toggle(id, &book(3)).unwrap();
        save_at(&path, &shelves).unwrap();
        let loaded = load_at(&path).unwrap();
        assert_eq!(loaded, shelves);
        let mut loaded = loaded;
        assert!(loaded.create("Ders").is_err());
        assert!(loaded.create("Bitenler").unwrap() > id);
        std::fs::write(
            &path,
            br#"{"version":1,"next_id":1,"shelves":[{"id":1,"name":"","books":[]}]}"#,
        )
        .unwrap();
        assert!(load_at(&path).is_err());
        std::fs::write(
            &path,
            br#"{"version":1,"next_id":1,"shelves":[{"id":1,"name":"A","books":["nope"]}]}"#,
        )
        .unwrap();
        assert!(load_at(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        let _ = std::fs::remove_dir(folder);
    }
}

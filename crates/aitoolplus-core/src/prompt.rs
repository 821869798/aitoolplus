//! Global prompt records per tool: CRUD, apply/取消, reorder, export.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub is_applied: bool,
    #[serde(default)]
    pub sort_index: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl PromptRecord {
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            content: content.into(),
            is_applied: false,
            sort_index: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Local::now().to_rfc3339();
    }
}

pub fn list(prompts: &[PromptRecord]) -> Vec<PromptRecord> {
    let mut v = prompts.to_vec();
    v.sort_by(|a, b| {
        a.sort_index
            .cmp(&b.sort_index)
            .then(a.created_at.cmp(&b.created_at))
    });
    v
}

pub fn create(prompts: &mut Vec<PromptRecord>, name: &str, content: &str) -> PromptRecord {
    let sort_index = prompts.iter().map(|p| p.sort_index).max().unwrap_or(-1) + 1;
    let mut rec = PromptRecord::new(name, content);
    rec.sort_index = sort_index;
    prompts.push(rec.clone());
    rec
}

pub fn delete(prompts: &mut Vec<PromptRecord>, id: &str) -> bool {
    let before = prompts.len();
    prompts.retain(|p| p.id != id);
    before != prompts.len()
}

pub fn update<F>(prompts: &mut [PromptRecord], id: &str, f: F) -> Option<PromptRecord>
where
    F: FnOnce(&mut PromptRecord),
{
    let rec = prompts.iter_mut().find(|p| p.id == id)?;
    f(rec);
    rec.touch();
    Some(rec.clone())
}

pub fn reorder(prompts: &mut Vec<PromptRecord>, from: usize, to: usize) {
    if from >= prompts.len() || to >= prompts.len() || from == to {
        return;
    }
    let item = prompts.remove(from);
    prompts.insert(to.min(prompts.len()), item);
    for (i, p) in prompts.iter_mut().enumerate() {
        p.sort_index = i as i32;
    }
}

/// Exactly one prompt may be applied at a time.
pub fn select(prompts: &mut [PromptRecord], id: &str) -> Option<PromptRecord> {
    let mut sel = None;
    for p in prompts.iter_mut() {
        p.is_applied = p.id == id;
        if p.is_applied {
            sel = Some(p.clone());
        }
    }
    sel
}

pub fn applied(prompts: &[PromptRecord]) -> Option<&PromptRecord> {
    prompts.iter().find(|p| p.is_applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs() -> Vec<PromptRecord> {
        let mut v = vec![];
        create(&mut v, "P1", "content 1");
        create(&mut v, "P2", "content 2");
        create(&mut v, "P3", "content 3");
        v[0].id = "1".into();
        v[1].id = "2".into();
        v[2].id = "3".into();
        v
    }

    #[test]
    fn crud_select_reorder() {
        let mut v = recs();
        assert_eq!(list(&v).len(), 3);

        let s = select(&mut v, "2").unwrap();
        assert_eq!(s.id, "2");
        assert_eq!(applied(&v).unwrap().id, "2");

        select(&mut v, "1");
        assert_eq!(applied(&v).unwrap().id, "1");

        reorder(&mut v, 2, 0);
        assert_eq!(list(&v)[0].id, "3");

        assert!(delete(&mut v, "1"));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn update_touches() {
        let mut v = recs();
        let before = v[0].updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(20));
        update(&mut v, "1", |p| p.name = "renamed".into()).unwrap();
        assert_eq!(v[0].name, "renamed");
        assert_ne!(v[0].updated_at, before);
    }
}

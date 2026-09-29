//! Undo history. Each step keeps the previous project revision; revisions share every entity that
//! did not change, so a step costs the size of what it touched.

use std::sync::Arc;
use std::time::Instant;

use crate::ids::{AssetId, ItemId, SequenceId};
use crate::model::Project;

/// What a committed step changed, derived by comparing revisions (semantic events for autosave,
/// scripting and the UI).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub items: Vec<ItemId>,
    pub assets: Vec<AssetId>,
    pub sequences: Vec<SequenceId>,
    pub settings: bool,
}

impl Changes {
    pub fn between(before: &Project, after: &Project) -> Changes {
        let mut c = Changes::default();
        for (id, v) in &after.items {
            if before
                .items
                .get(id)
                .is_none_or(|o| !Arc::ptr_eq(o, v) && o != v)
            {
                c.items.push(*id);
            }
        }
        for id in before.items.keys() {
            if !after.items.contains_key(id) {
                c.items.push(*id);
            }
        }
        for (id, v) in &after.assets {
            if before
                .assets
                .get(id)
                .is_none_or(|o| !Arc::ptr_eq(o, v) && o != v)
            {
                c.assets.push(*id);
            }
        }
        for id in before.assets.keys() {
            if !after.assets.contains_key(id) {
                c.assets.push(*id);
            }
        }
        for (id, v) in &after.sequences {
            if before
                .sequences
                .get(id)
                .is_none_or(|o| !Arc::ptr_eq(o, v) && o != v)
            {
                c.sequences.push(*id);
            }
        }
        for id in before.sequences.keys() {
            if !after.sequences.contains_key(id) {
                c.sequences.push(*id);
            }
        }
        c.settings = before.settings != after.settings || before.name != after.name;
        c
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
            && self.assets.is_empty()
            && self.sequences.is_empty()
            && !self.settings
    }
}

#[derive(Clone, Debug)]
pub struct Step {
    pub label: String,
    /// The project as it was before the step (for undo) or after it (for redo).
    pub project: Project,
    merge_key: Option<String>,
    at: Instant,
}

#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    limit: usize,
    /// Increments on every change of the current revision; used for dirty tracking.
    revision: u64,
    saved_revision: u64,
}

impl Default for History {
    fn default() -> Self {
        History::new(500)
    }
}

impl History {
    pub fn new(limit: usize) -> History {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            revision: 0,
            saved_revision: 0,
        }
    }

    /// Records that `current` replaced `before`. Consecutive steps with the same `merge_key` within
    /// a short time (dragging a value) collapse into one undo step.
    pub fn record(&mut self, label: impl Into<String>, before: Project, merge_key: Option<String>) {
        self.revision += 1;
        self.redo.clear();
        let now = Instant::now();
        if let (Some(key), Some(last)) = (&merge_key, self.undo.last_mut())
            && last.merge_key.as_ref() == Some(key)
            && now.duration_since(last.at).as_secs_f32() < 1.5
        {
            last.at = now;
            return;
        }
        self.undo.push(Step {
            label: label.into(),
            project: before,
            merge_key,
            at: now,
        });
        if self.undo.len() > self.limit {
            self.undo.remove(0);
            // the saved state may have scrolled out; it can no longer be reached by undo
            if self.saved_revision < self.revision.saturating_sub(self.limit as u64) {
                self.saved_revision = u64::MAX;
            }
        }
    }

    /// Ends a merge sequence so the next change starts a new step.
    pub fn seal(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.merge_key = None;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// Returns the revision to restore; `current` goes to the redo stack.
    pub fn undo(&mut self, current: &Project) -> Option<Project> {
        let step = self.undo.pop()?;
        self.revision += 1;
        self.redo.push(Step {
            label: step.label.clone(),
            project: current.clone(),
            merge_key: None,
            at: step.at,
        });
        Some(step.project)
    }

    pub fn redo(&mut self, current: &Project) -> Option<Project> {
        let step = self.redo.pop()?;
        self.revision += 1;
        self.undo.push(Step {
            label: step.label.clone(),
            project: current.clone(),
            merge_key: None,
            at: step.at,
        });
        Some(step.project)
    }

    /// Labels for the History panel, oldest first, and the index of the current state.
    pub fn labels(&self) -> (Vec<&str>, usize) {
        let mut v: Vec<&str> = self.undo.iter().map(|s| s.label.as_str()).collect();
        let current = v.len();
        v.extend(self.redo.iter().rev().map(|s| s.label.as_str()));
        (v, current)
    }

    /// Jumps to state `index` of `labels()` (0 = before the oldest step).
    pub fn jump(&mut self, index: usize, current: &Project) -> Option<Project> {
        let mut project = current.clone();
        let mut changed = false;
        while self.undo.len() > index {
            project = self.undo(&project)?;
            changed = true;
        }
        while self.undo.len() < index && !self.redo.is_empty() {
            project = self.redo(&project)?;
            changed = true;
        }
        changed.then_some(project)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.revision += 1;
        self.saved_revision = u64::MAX;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Whether anything changed since the last save (or since creation).
    pub fn is_dirty(&self) -> bool {
        self.saved_revision != self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SequenceSettings;

    #[test]
    fn undo_redo_and_merge() {
        let mut h = History::default();
        let mut p = Project::new("a");
        let before = p.clone();
        p.name = "b".into();
        h.record("Rename", before, None);
        let before = p.clone();
        p.add_sequence(p.root, "S", SequenceSettings::default());
        h.record("New Sequence", before, None);
        assert_eq!(h.labels().0, vec!["Rename", "New Sequence"]);

        // merged drag steps
        for i in 0..5 {
            let before = p.clone();
            p.name = format!("drag{i}");
            h.record("Drag", before, Some("x".into()));
        }
        assert_eq!(h.labels().0.len(), 3);
        p = h.undo(&p).unwrap();
        assert_eq!(p.name, "b", "one undo reverts the whole drag");
        p = h.undo(&p).unwrap();
        assert!(p.sequences.is_empty());
        p = h.redo(&p).unwrap();
        assert_eq!(p.sequences.len(), 1);
        let p2 = h.jump(0, &p).unwrap();
        assert_eq!(p2.name, "a");
    }

    #[test]
    fn dirty_tracking() {
        let mut h = History::default();
        let mut p = Project::new("a");
        h.mark_saved();
        assert!(!h.is_dirty());
        let before = p.clone();
        p.name = "b".into();
        h.record("Rename", before, None);
        assert!(h.is_dirty());
        let changes = Changes::between(&Project::new("a"), &p);
        assert!(changes.settings);
    }
}

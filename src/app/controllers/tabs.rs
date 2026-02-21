use fltk::app::Sender;
use fltk::text::TextBuffer;

use crate::app::domain::document::{Document, DocumentId};
use crate::app::domain::messages::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Grey,
}

impl GroupColor {
    pub const ALL: [GroupColor; 7] = [
        GroupColor::Red,
        GroupColor::Orange,
        GroupColor::Yellow,
        GroupColor::Green,
        GroupColor::Blue,
        GroupColor::Purple,
        GroupColor::Grey,
    ];

    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            GroupColor::Red => (220, 60, 60),
            GroupColor::Orange => (230, 150, 30),
            GroupColor::Yellow => (210, 190, 40),
            GroupColor::Green => (60, 170, 80),
            GroupColor::Blue => (60, 120, 220),
            GroupColor::Purple => (150, 80, 200),
            GroupColor::Grey => (140, 140, 140),
        }
    }

    pub fn to_rgb_dark(self) -> (u8, u8, u8) {
        match self {
            GroupColor::Red => (180, 50, 50),
            GroupColor::Orange => (190, 120, 25),
            GroupColor::Yellow => (170, 155, 35),
            GroupColor::Green => (50, 140, 65),
            GroupColor::Blue => (50, 100, 180),
            GroupColor::Purple => (125, 65, 165),
            GroupColor::Grey => (120, 120, 120),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            GroupColor::Red => "Red",
            GroupColor::Orange => "Orange",
            GroupColor::Yellow => "Yellow",
            GroupColor::Green => "Green",
            GroupColor::Blue => "Blue",
            GroupColor::Purple => "Purple",
            GroupColor::Grey => "Grey",
        }
    }

    pub fn from_str(s: &str) -> Option<GroupColor> {
        match s {
            "Red" => Some(GroupColor::Red),
            "Orange" => Some(GroupColor::Orange),
            "Yellow" => Some(GroupColor::Yellow),
            "Green" => Some(GroupColor::Green),
            "Blue" => Some(GroupColor::Blue),
            "Purple" => Some(GroupColor::Purple),
            "Grey" => Some(GroupColor::Grey),
            _ => None,
        }
    }
}

pub struct TabGroup {
    pub id: GroupId,
    pub name: String,
    pub color: GroupColor,
    pub collapsed: bool,
}

pub struct TabManager {
    documents: Vec<Document>,
    active_id: Option<DocumentId>,
    next_id: u64,
    untitled_counter: u32,
    sender: Sender<Message>,
    groups: Vec<TabGroup>,
    next_group_id: u64,
}

impl TabManager {
    pub fn new(sender: Sender<Message>) -> Self {
        Self {
            documents: Vec::new(),
            active_id: None,
            next_id: 1,
            untitled_counter: 0,
            sender,
            groups: Vec::new(),
            next_group_id: 1,
        }
    }

    fn next_document_id(&mut self) -> DocumentId {
        let id = DocumentId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn add_untitled(&mut self) -> DocumentId {
        self.untitled_counter += 1;
        let id = self.next_document_id();
        let doc = Document::new_untitled(id, self.untitled_counter, self.sender);
        self.documents.push(doc);
        self.active_id = Some(id);
        id
    }

    pub fn add_from_file(&mut self, path: String, content: &str) -> DocumentId {
        let id = self.next_document_id();
        let doc = Document::new_from_file(id, path, content, self.sender);
        self.documents.push(doc);
        self.active_id = Some(id);
        id
    }

    pub fn active_doc(&self) -> Option<&Document> {
        let active_id = self.active_id?;
        self.documents.iter().find(|d| d.id == active_id)
    }

    pub fn active_doc_mut(&mut self) -> Option<&mut Document> {
        let active_id = self.active_id?;
        self.documents.iter_mut().find(|d| d.id == active_id)
    }

    pub fn active_buffer(&self) -> Option<TextBuffer> {
        self.active_doc().map(|d| d.buffer.clone())
    }

    pub fn set_active(&mut self, id: DocumentId) {
        if self.documents.iter().any(|d| d.id == id) {
            self.active_id = Some(id);
        }
    }

    /// Remove a document by id. Activates the nearest neighbor.
    /// Cleans up the buffer to free memory immediately.
    pub fn remove(&mut self, id: DocumentId) {
        let idx = match self.documents.iter().position(|d| d.id == id) {
            Some(i) => i,
            None => return,
        };
        let mut doc = self.documents.remove(idx);
        doc.cleanup();

        // Activate nearest neighbor
        if self.active_id == Some(id) {
            if self.documents.is_empty() {
                self.active_id = None;
            } else {
                let new_idx = if idx >= self.documents.len() {
                    self.documents.len() - 1
                } else {
                    idx
                };
                self.active_id = Some(self.documents[new_idx].id);
            }
        }

    }

    /// Move a tab from one index to another.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.documents.len() {
            return;
        }
        let source_group = self.documents[from].group_id;

        // Clamp `to` so it doesn't land inside a group the source doesn't belong to.
        // `to` is an insertion index (0..=len), not a tab index.
        let to = to.min(self.documents.len());
        let adjusted = self.clamp_insert_outside_foreign_group(to, source_group);

        let doc = self.documents.remove(from);
        // After removal, insertion indices >= from shift down by 1
        let insert_at = if adjusted > from { adjusted - 1 } else { adjusted };
        let insert_at = insert_at.min(self.documents.len());
        if insert_at == from && self.documents.len() > from {
            // No actual move needed — re-insert at original position
            self.documents.insert(from, doc);
            return;
        }
        self.documents.insert(insert_at, doc);
    }

    /// If `pos` falls inside a contiguous run of a group that `source_group` doesn't belong to,
    /// snap it to the nearest boundary of that group.
    fn clamp_insert_outside_foreign_group(&self, pos: usize, source_group: Option<GroupId>) -> usize {
        if pos == 0 || pos >= self.documents.len() {
            return pos;
        }
        // Check the group of the tabs on either side of the insertion point
        let left_group = self.documents[pos - 1].group_id;
        let right_group = self.documents[pos].group_id;

        // Only a problem if both sides are the same group AND source isn't part of it
        if let Some(gid) = left_group
            && left_group == right_group && source_group != Some(gid) {
                // Find the group boundary — snap to the end of this group
                let group_end = self.documents.iter()
                    .rposition(|d| d.group_id == Some(gid))
                    .map_or(pos, |i| i + 1);
                return group_end;
            }
        pos
    }

    /// Move all tabs belonging to a group to a new position.
    pub fn move_group(&mut self, group_id: GroupId, to: usize) {
        // Collect the indices of all tabs in this group (in order)
        let group_indices: Vec<usize> = self.documents.iter()
            .enumerate()
            .filter(|(_, d)| d.group_id == Some(group_id))
            .map(|(i, _)| i)
            .collect();

        if group_indices.is_empty() {
            return;
        }

        // Extract group docs (remove from back to front to keep indices stable)
        let mut group_docs: Vec<Document> = Vec::new();
        for &idx in group_indices.iter().rev() {
            group_docs.push(self.documents.remove(idx));
        }
        group_docs.reverse();

        // Adjust insertion point after removals
        let first_old = group_indices[0];
        let count = group_docs.len();
        let insert_at = if to > first_old {
            // Indices shifted left by the number of removed items before `to`
            let removed_before = group_indices.iter().filter(|&&i| i < to).count();
            (to - removed_before).min(self.documents.len())
        } else {
            to.min(self.documents.len())
        };

        // Clamp so we don't split a foreign group
        let insert_at = self.clamp_insert_outside_foreign_group(insert_at, Some(group_id));
        let insert_at = insert_at.min(self.documents.len());

        // Re-insert all group docs at the target position
        for (i, doc) in group_docs.into_iter().enumerate() {
            self.documents.insert(insert_at + i, doc);
        }

        // Also reorder the group in the groups list to maintain visual order
        // (not strictly necessary but keeps things consistent)
        let _ = count; // used above
    }

    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    pub fn count(&self) -> usize {
        self.documents.len()
    }

    pub fn active_id(&self) -> Option<DocumentId> {
        self.active_id
    }

    /// Find a document by file path
    pub fn find_by_path(&self, path: &str) -> Option<DocumentId> {
        self.documents
            .iter()
            .find(|d| d.file_path.as_deref() == Some(path))
            .map(|d| d.id)
    }

    pub fn doc_by_id(&self, id: DocumentId) -> Option<&Document> {
        self.documents.iter().find(|d| d.id == id)
    }

    pub fn doc_by_id_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        self.documents.iter_mut().find(|d| d.id == id)
    }


    /// Get the next document id (for tab cycling)
    pub fn next_doc_id(&self) -> Option<DocumentId> {
        let active_id = self.active_id?;
        let idx = self.documents.iter().position(|d| d.id == active_id)?;
        let next_idx = (idx + 1) % self.documents.len();
        Some(self.documents[next_idx].id)
    }

    /// Get the previous document id (for tab cycling)
    pub fn prev_doc_id(&self) -> Option<DocumentId> {
        let active_id = self.active_id?;
        let idx = self.documents.iter().position(|d| d.id == active_id)?;
        let prev_idx = if idx == 0 {
            self.documents.len() - 1
        } else {
            idx - 1
        };
        Some(self.documents[prev_idx].id)
    }

    // --- Tab Group methods ---

    pub fn groups(&self) -> &[TabGroup] {
        &self.groups
    }

    fn next_group_color(&self) -> GroupColor {
        GroupColor::ALL[self.groups.len() % GroupColor::ALL.len()]
    }

    pub fn create_group(&mut self, tab_ids: &[DocumentId]) -> GroupId {
        let id = GroupId(self.next_group_id);
        self.next_group_id += 1;
        let color = self.next_group_color();
        self.groups.push(TabGroup {
            id,
            name: String::new(),
            color,
            collapsed: false,
        });
        for &doc_id in tab_ids {
            if let Some(doc) = self.documents.iter_mut().find(|d| d.id == doc_id) {
                doc.group_id = Some(id);
            }
        }
        id
    }

    pub fn delete_group(&mut self, id: GroupId) {
        for doc in &mut self.documents {
            if doc.group_id == Some(id) {
                doc.group_id = None;
            }
        }
        self.groups.retain(|g| g.id != id);
    }

    /// Returns the list of document IDs in the group (for closing).
    pub fn group_doc_ids(&self, id: GroupId) -> Vec<DocumentId> {
        self.documents
            .iter()
            .filter(|d| d.group_id == Some(id))
            .map(|d| d.id)
            .collect()
    }

    pub fn rename_group(&mut self, id: GroupId, name: String) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.name = name;
        }
    }

    pub fn recolor_group(&mut self, id: GroupId, color: GroupColor) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.color = color;
        }
    }

    pub fn set_tab_group(&mut self, doc_id: DocumentId, group_id: Option<GroupId>) {
        if let Some(doc) = self.documents.iter_mut().find(|d| d.id == doc_id) {
            doc.group_id = group_id;
        }
        // Move the tab to sit next to its group members
        if let Some(gid) = group_id
            && let Some(src) = self.documents.iter().position(|d| d.id == doc_id) {
                // Find the last tab in this group (excluding the one we're moving)
                let last_in_group = self.documents.iter().enumerate()
                    .filter(|(i, d)| d.group_id == Some(gid) && *i != src)
                    .map(|(i, _)| i)
                    .next_back();
                if let Some(dest) = last_in_group {
                    let doc = self.documents.remove(src);
                    // If we removed before dest, dest shifted left by 1
                    let insert_at = if src < dest { dest } else { dest + 1 };
                    self.documents.insert(insert_at, doc);
                }
            }
    }

    pub fn toggle_group_collapsed(&mut self, id: GroupId) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.collapsed = !g.collapsed;
        }
    }

    pub fn group_by_id(&self, id: GroupId) -> Option<&TabGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Restore groups from session data. Returns a mapping of session group index -> GroupId.
    pub fn restore_groups(&mut self, groups: Vec<TabGroup>) -> Vec<GroupId> {
        let mut ids = Vec::new();
        for g in groups {
            let id = GroupId(self.next_group_id);
            self.next_group_id += 1;
            ids.push(id);
            self.groups.push(TabGroup {
                id,
                name: g.name,
                color: g.color,
                collapsed: g.collapsed,
            });
        }
        ids
    }
}

// Note: TabManager tests require FLTK initialization which doesn't work well
// with parallel test execution. Tests for TabManager logic are covered via
// integration testing. Unit tests for pure functions like GroupColor are below.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_color_to_rgb() {
        let color = GroupColor::Blue;
        let (r, g, b) = color.to_rgb();
        assert!(r < g && g < b); // Blue should have highest blue component
    }

    #[test]
    fn test_group_color_to_rgb_dark() {
        let color = GroupColor::Green;
        let (r, g, b) = color.to_rgb_dark();
        // Dark mode colors should be different from light mode
        let (lr, lg, lb) = color.to_rgb();
        assert!(r != lr || g != lg || b != lb);
    }
}

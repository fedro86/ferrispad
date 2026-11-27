use std::collections::HashMap;
use std::path::Path;

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

    #[allow(clippy::should_implement_trait)]
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

    /// Add an empty untitled document, numbered with the lowest number no
    /// open untitled document holds (T0091): the first is "Untitled", and a
    /// closed or saved one frees its number for the next.
    pub fn add_untitled(&mut self) -> DocumentId {
        let number = self.lowest_free_untitled_number();
        let id = self.next_document_id();
        let doc = Document::new_untitled(id, number, self.sender);
        self.documents.push(doc);
        self.active_id = Some(id);
        id
    }

    /// The smallest `n >= 1` that no open untitled document holds.
    pub fn lowest_free_untitled_number(&self) -> u32 {
        lowest_free_number(self.documents.iter().filter_map(Document::untitled_number))
    }

    pub fn add_from_file(&mut self, path: String, content: &str) -> DocumentId {
        let id = self.next_document_id();
        let doc = Document::new_from_file(id, path, content, self.sender);
        self.documents.push(doc);
        self.active_id = Some(id);
        id
    }

    /// Add a document from a pre-populated TextBuffer (memory-optimized path).
    ///
    /// Use this for large files where the buffer was streamed directly
    /// to avoid a full copy of the content.
    pub fn add_from_buffer(
        &mut self,
        path: String,
        buffer: TextBuffer,
        skip_style_init: bool,
    ) -> DocumentId {
        let id = self.next_document_id();
        let doc = Document::new_from_buffer(id, path, buffer, self.sender, skip_style_init);
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
        let insert_at = if adjusted > from {
            adjusted - 1
        } else {
            adjusted
        };
        let insert_at = insert_at.min(self.documents.len());
        if insert_at == from && self.documents.len() > from {
            // No actual move needed — re-insert at original position
            self.documents.insert(from, doc);
            return;
        }
        self.documents.insert(insert_at, doc);
    }

    /// Move a tab to a new position and optionally change its group atomically.
    /// This bypasses clamp_insert_outside_foreign_group when a target group is specified,
    /// allowing the tab to be inserted inside the target group.
    pub fn move_tab_to_group(
        &mut self,
        doc_id: DocumentId,
        to: usize,
        target_group: Option<GroupId>,
    ) {
        let from = match self.documents.iter().position(|d| d.id == doc_id) {
            Some(i) => i,
            None => return,
        };

        // Remove the document
        let mut doc = self.documents.remove(from);

        // Set the new group
        doc.group_id = target_group;

        // Calculate insertion point (no clamping needed since we're joining the target group)
        let insert_at = if to > from { to - 1 } else { to };
        let insert_at = insert_at.min(self.documents.len());

        self.documents.insert(insert_at, doc);
    }

    /// If `pos` falls inside a contiguous run of a group that `source_group` doesn't belong to,
    /// snap it to the nearest boundary of that group.
    fn clamp_insert_outside_foreign_group(
        &self,
        pos: usize,
        source_group: Option<GroupId>,
    ) -> usize {
        if pos == 0 || pos >= self.documents.len() {
            return pos;
        }
        // Check the group of the tabs on either side of the insertion point
        let left_group = self.documents[pos - 1].group_id;
        let right_group = self.documents[pos].group_id;

        // Only a problem if both sides are the same group AND source isn't part of it
        if let Some(gid) = left_group
            && left_group == right_group
            && source_group != Some(gid)
        {
            // Find the group boundary — snap to the end of this group
            let group_end = self
                .documents
                .iter()
                .rposition(|d| d.group_id == Some(gid))
                .map_or(pos, |i| i + 1);
            return group_end;
        }
        pos
    }

    /// Move all tabs belonging to a group to a new position.
    pub fn move_group(&mut self, group_id: GroupId, to: usize) {
        // Collect the indices of all tabs in this group (in order)
        let group_indices: Vec<usize> = self
            .documents
            .iter()
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

    /// Remove all documents and groups, cleaning up buffers.
    /// Leaves the tab manager empty (no active document).
    pub fn clear(&mut self) {
        for doc in &mut self.documents {
            doc.cleanup();
        }
        self.documents.clear();
        self.active_id = None;
        self.groups.clear();
    }

    /// Find a document by file path
    pub fn find_by_path(&self, path: &str) -> Option<DocumentId> {
        self.documents
            .iter()
            .find(|d| d.file_path.as_deref() == Some(path))
            .map(|d| d.id)
    }

    /// The tab label of document `id` (`tab_labels`).
    pub fn tab_label(&self, id: DocumentId) -> Option<String> {
        let i = self.documents.iter().position(|d| d.id == id)?;
        tab_labels(&self.documents).into_iter().nth(i)
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
            && let Some(src) = self.documents.iter().position(|d| d.id == doc_id)
        {
            // Find the last tab in this group (excluding the one we're moving)
            let last_in_group = self
                .documents
                .iter()
                .enumerate()
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

    /// Expand the group containing this document, if it's collapsed.
    pub fn expand_group_of(&mut self, doc_id: DocumentId) {
        if let Some(group_id) = self
            .documents
            .iter()
            .find(|d| d.id == doc_id)
            .and_then(|d| d.group_id)
            && let Some(g) = self.groups.iter_mut().find(|g| g.id == group_id)
        {
            g.collapsed = false;
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

    /// Set tab distance (in characters) for all document buffers.
    pub fn set_all_tab_distance(&mut self, distance: i32) {
        for doc in &mut self.documents {
            // Save dirty state: FLTK's set_tab_distance() fires the modify
            // callback (with deleted=len, inserted=len) to force a redisplay,
            // which would incorrectly mark clean documents as dirty.
            let was_dirty = doc.is_dirty();
            doc.buffer.set_tab_distance(distance);
            if !was_dirty {
                doc.mark_clean();
            }
        }
    }
}

// Note: TabManager tests require FLTK initialization which doesn't work well
// with parallel test execution. Tests for TabManager logic are covered via
// integration testing. Unit tests for pure functions like GroupColor are below.

/// Tab labels for `documents`, in order: the display name, plus the
/// minimal parent folders ("main.rs (src)", VS Code style) when another
/// document has the same name.
///
/// Labels are computed from the current paths and names whenever they are
/// shown, never cached, so no change of a path or name (open, close, new
/// tab, Save As, tail or chunk views) can leave two tabs with the same
/// label until the next recompute (T0112). A document without a path (an
/// untitled one) keeps its name, which T0091 numbering makes unique; only
/// files take folder suffixes, chosen to be unique among the files.
pub fn tab_labels(documents: &[Document]) -> Vec<String> {
    let entries: Vec<(&str, Option<&str>)> = documents
        .iter()
        .map(|d| (d.display_name.as_str(), d.file_path.as_deref()))
        .collect();
    labels_for(&entries)
}

/// The label rule of [`tab_labels`] on (display name, path) pairs: pure, so
/// it is tested without building documents.
fn labels_for(entries: &[(&str, Option<&str>)]) -> Vec<String> {
    let mut labels: Vec<String> = entries.iter().map(|(name, _)| name.to_string()).collect();
    let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (name, _)) in entries.iter().enumerate() {
        groups.entry(name).or_default().push(i);
    }
    for (name, indices) in groups {
        if indices.len() < 2 {
            continue;
        }
        let files: Vec<(usize, &str)> = indices
            .iter()
            .filter_map(|&i| entries[i].1.map(|p| (i, p)))
            .collect();
        for (i, suffix) in files.iter().map(|&(i, _)| i).zip(folder_suffixes(&files)) {
            if !suffix.is_empty() {
                labels[i] = format!("{name} ({suffix})");
            }
        }
    }
    labels
}

/// The shortest trailing run of parent folders that tells `files` apart
/// (at least one folder), else each file's whole parent path.
fn folder_suffixes(files: &[(usize, &str)]) -> Vec<String> {
    let rev_segments: Vec<Vec<String>> = files
        .iter()
        .map(|(_, p)| {
            Path::new(p)
                .parent()
                .unwrap_or(Path::new(""))
                .components()
                .rev()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect()
        })
        .collect();
    let max_depth = rev_segments.iter().map(Vec::len).max().unwrap_or(0);
    for depth in 1..=max_depth {
        let suffixes: Vec<String> = rev_segments
            .iter()
            .map(|segs| {
                segs.iter()
                    .take(depth)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        let unique = suffixes
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == suffixes.len();
        if unique {
            return suffixes;
        }
    }
    files
        .iter()
        .map(|(_, p)| {
            Path::new(p)
                .parent()
                .map(|pp| pp.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .collect()
}

/// The smallest `n >= 1` not in `used` (T0091): the number of the next
/// untitled document.
pub fn lowest_free_number(used: impl IntoIterator<Item = u32>) -> u32 {
    let used: std::collections::BTreeSet<u32> = used.into_iter().collect();
    let mut n = 1;
    while used.contains(&n) {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- T0091: untitled numbering (the rule; the TabManager wiring runs
    // on the main thread, `main_thread_checks`) ---

    #[test]
    fn untitled_lowest_free_number() {
        assert_eq!(lowest_free_number([]), 1, "none open: \"Untitled\"");
        assert_eq!(lowest_free_number([1, 2, 3]), 4);
        assert_eq!(lowest_free_number([1, 3]), 2, "a closed one is reused");
        assert_eq!(lowest_free_number([2, 3]), 1);
    }

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

    // --- T0112: tab labels (the rule; the TabManager wiring runs on the
    // main thread, `main_thread_checks::tab_label_follows_paths_and_closes`) ---

    fn labels(entries: &[(&str, Option<&str>)]) -> Vec<String> {
        labels_for(entries)
    }

    /// A file literally named "Untitled" next to an untitled document: two
    /// distinct labels, in either order, and no empty "()" suffix.
    #[test]
    fn untitled_label_distinct_from_file_named_untitled() {
        let file = ("Untitled", Some("/home/u/notes/Untitled"));
        let new = ("Untitled", None);
        assert_eq!(
            labels(&[file, new]),
            ["Untitled (notes)", "Untitled"],
            "file opened first, then a new tab"
        );
        assert_eq!(
            labels(&[new, file]),
            ["Untitled", "Untitled (notes)"],
            "a new tab first, then the file"
        );
        assert_eq!(labels(&[new]), ["Untitled"], "the file closed");
    }

    /// Two files named like an untitled document, in different folders:
    /// they disambiguate among themselves; the untitled tabs keep their
    /// names.
    #[test]
    fn untitled_label_with_two_files_of_its_name() {
        assert_eq!(
            labels(&[
                ("Untitled", None),
                ("Untitled 2", None),
                ("Untitled 2", Some("/a/Untitled 2")),
                ("Untitled 2", Some("/b/Untitled 2")),
            ]),
            ["Untitled", "Untitled 2", "Untitled 2 (a)", "Untitled 2 (b)"]
        );
    }

    /// Save As gives a tab the name of another open file: both labels
    /// disambiguate at once.
    #[test]
    fn save_as_same_name_labels_distinct() {
        assert_eq!(
            labels(&[
                ("main.rs", Some("/y/main.rs")),
                ("main.rs", Some("/x/main.rs"))
            ]),
            ["main.rs (y)", "main.rs (x)"]
        );
    }

    /// Guard: labels of ordinary files, as before T0112.
    #[test]
    fn file_labels_unchanged() {
        assert_eq!(
            labels(&[
                ("lib.rs", Some("/p/lib.rs")),
                ("main.rs", Some("/p/a/main.rs")),
                ("main.rs", Some("/p/b/main.rs")),
                ("mod.rs", Some("/x/src/mod.rs")),
                ("mod.rs", Some("/y/src/mod.rs")),
                ("Untitled", None),
            ]),
            [
                "lib.rs",
                "main.rs (a)",
                "main.rs (b)",
                "mod.rs (x/src)",
                "mod.rs (y/src)",
                "Untitled"
            ]
        );
        assert_eq!(
            labels(&[("main.rs", Some("/p/a/main.rs"))]),
            ["main.rs"],
            "the other main.rs closed"
        );
    }
}

/// Checks that build documents (hence FLTK `TextBuffer`s), so they must run
/// on the process main thread: on macOS the first `Fl::lock()` opens the
/// display through AppKit, which aborts off the main thread, where libtest
/// runs unit tests. Called by the `harness = false` test binary
/// `tests/fltk_main_thread.rs` (T0089); not part of the public API.
#[doc(hidden)]
pub mod main_thread_checks {
    use super::*;

    fn tabs() -> TabManager {
        let (sender, _receiver) = fltk::app::channel::<Message>();
        TabManager::new(sender)
    }

    fn name(tabs: &TabManager, id: DocumentId) -> String {
        tabs.doc_by_id(id).unwrap().display_name.clone()
    }

    /// Startup creates the default untitled document and removes it for the
    /// start page; the user's first new tab must still be "Untitled".
    pub fn untitled_first_new_after_startup_is_untitled() {
        let mut tabs = tabs();
        let default = tabs.add_untitled();
        tabs.remove(default);
        let first = tabs.add_untitled();
        assert_eq!(name(&tabs, first), "Untitled");
        let second = tabs.add_untitled();
        assert_eq!(name(&tabs, second), "Untitled 2");
    }

    pub fn untitled_reuses_lowest_free_number() {
        let mut tabs = tabs();
        let ids: Vec<_> = (0..3).map(|_| tabs.add_untitled()).collect();
        let names: Vec<_> = ids.iter().map(|&id| name(&tabs, id)).collect();
        assert_eq!(names, ["Untitled", "Untitled 2", "Untitled 3"]);
        tabs.remove(ids[1]);
        let again = tabs.add_untitled();
        assert_eq!(name(&tabs, again), "Untitled 2");
        for id in [ids[0], ids[2], again] {
            tabs.remove(id);
        }
        let fresh = tabs.add_untitled();
        assert_eq!(name(&tabs, fresh), "Untitled");
    }

    /// T0112: `TabManager::tab_label` follows the documents' current paths
    /// and names, with nothing cached: a new tab, a Save As done in place
    /// (as `file.rs` does) and a close all show at once.
    pub fn tab_label_follows_paths_and_closes() {
        let mut tabs = tabs();
        let label = |tabs: &TabManager, id| tabs.tab_label(id).unwrap();
        let file = tabs.add_from_file("/home/u/notes/Untitled".to_string(), "x");
        let new = tabs.add_untitled();
        assert_eq!(label(&tabs, file), "Untitled (notes)");
        assert_eq!(label(&tabs, new), "Untitled");
        let open = tabs.add_from_file("/y/main.rs".to_string(), "");
        let other = tabs.add_from_file("/x/notes.txt".to_string(), "");
        let doc = tabs.doc_by_id_mut(other).unwrap();
        doc.file_path = Some("/x/main.rs".to_string());
        doc.update_display_name();
        assert_eq!(label(&tabs, open), "main.rs (y)");
        assert_eq!(label(&tabs, other), "main.rs (x)");
        tabs.remove(file);
        tabs.remove(other);
        assert_eq!(label(&tabs, new), "Untitled");
        assert_eq!(label(&tabs, open), "main.rs");
    }

    /// Saving an untitled document gives it a path: its number is free.
    pub fn untitled_saved_doc_frees_its_number() {
        let mut tabs = tabs();
        let first = tabs.add_untitled();
        let doc = tabs.doc_by_id_mut(first).unwrap();
        doc.file_path = Some("/tmp/t0091/notes.txt".to_string());
        doc.update_display_name();
        assert_eq!(name(&tabs, first), "notes.txt");
        let next = tabs.add_untitled();
        assert_eq!(name(&tabs, next), "Untitled");
    }
}

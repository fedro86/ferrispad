use fltk::text::TextBuffer;

use super::document::{Document, DocumentId};

pub struct TabManager {
    documents: Vec<Document>,
    active_id: Option<DocumentId>,
    next_id: u64,
    untitled_counter: u32,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            active_id: None,
            next_id: 1,
            untitled_counter: 0,
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
        let doc = Document::new_untitled(id, self.untitled_counter);
        self.documents.push(doc);
        self.active_id = Some(id);
        id
    }

    pub fn add_from_file(&mut self, path: String, content: &str) -> DocumentId {
        let id = self.next_document_id();
        let doc = Document::new_from_file(id, path, content);
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
}

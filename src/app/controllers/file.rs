use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use super::preview::{PreviewController, wrap_html_for_preview};
use super::tabs::TabManager;
use crate::app::domain::document::DocumentId;
use crate::app::domain::document::PartialFileInfo;
use crate::app::domain::settings::AppSettings;
use crate::app::infrastructure::buffer::buffer_text_no_leak;
use crate::app::plugins::{HookResult, PluginHook, PluginManager};
use crate::app::services::file_size::{
    FileSizeCheck, TAIL_LINE_COUNT, check_file_size, read_chunk, read_tail, save_partial,
};
use crate::ui::dialogs::large_file::{
    StreamLoadResult, TooLargeAction, load_to_buffer_with_progress, show_file_too_large_dialog,
    show_large_file_warning,
};
use crate::ui::file_dialogs::{native_open_dialog, native_open_multi_dialog, native_save_dialog};

/// Files larger than this threshold defer plugin hooks / tree refresh
/// to the next event loop iteration so the UI stays responsive.
const DEFERRED_THRESHOLD: usize = 512_000; // 500 KB

/// Actions that FileController cannot execute directly because they
/// require access to AppState fields (editor, highlight, widgets, etc.).
/// AppState dispatches these after each FileController call.
pub enum FileAction {
    SwitchToDocument(DocumentId),
    RebuildTabBar,
    DetectAndHighlight(DocumentId, String),
    /// Re-bind style buffer to editor after detect_and_highlight (file_save_as).
    BindHighlightData(DocumentId),
    UpdateWindowTitle,
    UpdateMenusForFileType,
    /// Run plugin on_document_open hooks with content (normal files).
    RunOpenHooks {
        path: String,
        content: String,
    },
    /// Defer plugin hooks to next event loop iteration (large files).
    DeferOpenHooks {
        path: String,
        content: String,
    },
    /// Process lint result from save hooks.
    ProcessLintResult(Box<HookResult>),
    /// Update markdown preview file if applicable.
    UpdatePreviewFile {
        doc_id: u64,
        path: String,
        text: String,
    },
    /// Tell the user a file operation failed (themed error popup).
    ShowError(String),
}

/// Kind of external change detected for an open document.
pub enum ExternalChange {
    /// File content changed on disk (mtime differs).
    Modified,
    /// File no longer exists on disk.
    Deleted,
}

/// Manages file I/O operations (open, save, new).
///
/// Holds file-dialog state (`last_open_directory`). All cross-cutting
/// side effects (tab switching, highlighting, plugin hooks) are returned
/// as `Vec<FileAction>` for AppState to dispatch.
#[derive(Default)]
pub struct FileController {
    pub last_open_directory: Option<String>,
}

impl FileController {
    // ===== Public API =====

    pub fn open_file(
        &mut self,
        path: String,
        tab_manager: &mut TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        theme_bg: (u8, u8, u8),
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        // Remember the parent directory for future open/save dialogs
        let path_ref = std::path::Path::new(&path);
        if let Some(parent) = path_ref.parent() {
            self.last_open_directory = Some(parent.to_string_lossy().to_string());
        }

        // Pre-flight size check to prevent crashes on huge files
        let warning_mb = settings.borrow().large_file_warning_mb as u64;
        let max_editable_mb = settings.borrow().max_editable_size_mb as u64;
        match check_file_size(path_ref, warning_mb, max_editable_mb) {
            Ok(FileSizeCheck::TooLarge(size)) => {
                let max_mb = settings.borrow().max_editable_size_mb;
                match show_file_too_large_dialog(path_ref, size, theme_bg, max_mb) {
                    TooLargeAction::Cancel => return vec![],
                    TooLargeAction::ViewReadOnly => {
                        let filename = path_ref
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        let (code_font, code_size) = {
                            let s = settings.borrow();
                            (s.current_font(), s.font_size as i32)
                        };
                        if let Some(req) = crate::ui::dialogs::readonly_viewer::show_readonly_viewer(
                            path_ref, theme_bg, code_font, code_size,
                        ) {
                            // User clicked "Open" — content already extracted from mmap
                            return self.open_chunk_content(
                                path,
                                req.content,
                                &filename,
                                req.start_line,
                                req.end_line,
                                req.start_byte,
                                req.end_byte,
                                tab_manager,
                                tabs_enabled,
                            );
                        }
                        return vec![];
                    }
                    TooLargeAction::OpenTail => {
                        let filename = path_ref
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        match read_tail(path_ref, TAIL_LINE_COUNT) {
                            Ok((content, start_byte)) => {
                                return self.open_tail_content(
                                    path,
                                    content,
                                    &filename,
                                    start_byte,
                                    tab_manager,
                                    tabs_enabled,
                                );
                            }
                            Err(e) => {
                                return vec![FileAction::ShowError(format!(
                                    "Failed to read file tail: {}",
                                    e
                                ))];
                            }
                        }
                    }
                    TooLargeAction::OpenChunk(start_line, end_line) => {
                        let filename = path_ref
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        match read_chunk(path_ref, start_line, end_line) {
                            Ok((content, start_byte, end_byte)) => {
                                return self.open_chunk_content(
                                    path,
                                    content,
                                    &filename,
                                    start_line,
                                    end_line,
                                    start_byte,
                                    end_byte,
                                    tab_manager,
                                    tabs_enabled,
                                );
                            }
                            Err(e) => {
                                return vec![FileAction::ShowError(format!(
                                    "Failed to read file chunk: {}",
                                    e
                                ))];
                            }
                        }
                    }
                }
            }
            Ok(FileSizeCheck::Large(size)) => {
                if !show_large_file_warning(path_ref, size, theme_bg) {
                    return vec![]; // User cancelled
                }
                // Stream directly to TextBuffer to avoid ~2x memory
                match load_to_buffer_with_progress(path_ref, size, theme_bg) {
                    StreamLoadResult::Success(buffer) => {
                        return self.open_large_file_buffer(
                            path,
                            buffer,
                            tab_manager,
                            tabs_enabled,
                        );
                    }
                    StreamLoadResult::Cancelled => return vec![],
                    StreamLoadResult::Error(e) => {
                        return vec![FileAction::ShowError(format!("Error opening file: {}", e))];
                    }
                }
            }
            Ok(FileSizeCheck::Normal(_)) => {
                // Normal file, proceed with direct read
            }
            Err(e) => {
                eprintln!("[file] Warning: could not check file size: {}", e);
            }
        }

        match fs::read_to_string(&path) {
            Ok(content) => self.open_file_content(path, content, tab_manager, tabs_enabled),
            Err(e) => {
                vec![FileAction::ShowError(format!("Error opening file: {}", e))]
            }
        }
    }

    /// Open a path given on the command line. A missing file whose folder
    /// exists starts a new, empty, unmodified document bound to that path, so
    /// Save creates it without asking for a name (like gedit or vim).
    pub fn open_cli_path(
        &mut self,
        path: String,
        tab_manager: &mut TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        theme_bg: (u8, u8, u8),
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        match resolve_cli_open_target(&path) {
            CliOpenTarget::Open(path) => {
                self.open_file(path, tab_manager, settings, theme_bg, tabs_enabled)
            }
            CliOpenTarget::NewFile(path) => {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    self.last_open_directory = Some(parent.to_string_lossy().to_string());
                }
                self.open_file_content(path, String::new(), tab_manager, tabs_enabled)
            }
            CliOpenTarget::Invalid(message) => vec![FileAction::ShowError(message)],
        }
    }

    pub fn file_new(&self, tab_manager: &mut TabManager, tabs_enabled: bool) -> Vec<FileAction> {
        if tabs_enabled {
            let id = tab_manager.add_untitled();
            vec![FileAction::SwitchToDocument(id), FileAction::RebuildTabBar]
        } else {
            if let Some(doc) = tab_manager.active_doc_mut() {
                doc.buffer.set_text("");
                doc.has_unsaved_changes.set(false);
                doc.file_path = None;
                doc.display_name = "Untitled".to_string();
                doc.syntax_name = None;
                doc.checkpoints.clear();
                doc.style_buffer.set_text("");
            }
            vec![
                FileAction::UpdateWindowTitle,
                FileAction::UpdateMenusForFileType,
            ]
        }
    }

    pub fn file_open(
        &mut self,
        tab_manager: &mut TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        theme_bg: (u8, u8, u8),
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        let dir = self.last_open_directory.as_deref();
        if tabs_enabled {
            let paths = native_open_multi_dialog(dir, theme_bg);
            let mut actions = Vec::new();
            for path in paths {
                actions.extend(self.open_file(path, tab_manager, settings, theme_bg, tabs_enabled));
            }
            actions
        } else if let Some(path) = native_open_dialog(dir, theme_bg) {
            self.open_file(path, tab_manager, settings, theme_bg, tabs_enabled)
        } else {
            vec![]
        }
    }

    pub fn file_save(
        &mut self,
        tab_manager: &mut TabManager,
        plugins: &PluginManager,
        theme_bg: (u8, u8, u8),
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        let (file_path, text, doc_id, partial_info) = {
            if let Some(doc) = tab_manager.active_doc() {
                (
                    doc.file_path.clone(),
                    buffer_text_no_leak(&doc.buffer),
                    doc.id.0,
                    doc.partial_info.clone(),
                )
            } else {
                return vec![];
            }
        };

        if let Some(ref path) = file_path {
            // Call plugin hook - plugins can modify content before save
            let hook_result = plugins.call_hook(PluginHook::OnDocumentSave {
                path: path.clone(),
                content: text.clone(),
            });
            let text_to_save = hook_result.modified_content.unwrap_or(text);

            let save_result = match &partial_info {
                PartialFileInfo::Full => fs::write(path, &text_to_save),
                PartialFileInfo::Tail { start_byte } => {
                    save_partial(
                        std::path::Path::new(path),
                        &text_to_save,
                        *start_byte,
                        // end_byte = file size (tail extends to EOF)
                        fs::metadata(path).map(|m| m.len()).unwrap_or(*start_byte),
                    )
                }
                PartialFileInfo::Chunk {
                    start_byte,
                    end_byte,
                } => save_partial(
                    std::path::Path::new(path),
                    &text_to_save,
                    *start_byte,
                    *end_byte,
                ),
            };

            match save_result {
                Ok(_) => {
                    if let Some(doc) = tab_manager.active_doc_mut() {
                        doc.mark_clean();
                        doc.disk_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
                    }

                    // Call lint hook after successful save
                    let lint_result = plugins.call_hook(PluginHook::OnDocumentLint {
                        path: path.clone(),
                        content: text_to_save.clone(),
                    });

                    vec![
                        FileAction::UpdateWindowTitle,
                        FileAction::RebuildTabBar,
                        FileAction::UpdatePreviewFile {
                            doc_id,
                            path: path.clone(),
                            text: text_to_save,
                        },
                        FileAction::ProcessLintResult(Box::new(lint_result)),
                    ]
                }
                Err(e) => {
                    vec![FileAction::ShowError(format!("Error saving file: {}", e))]
                }
            }
        } else {
            self.file_save_as(tab_manager, plugins, theme_bg, tabs_enabled)
        }
    }

    pub fn file_save_as(
        &mut self,
        tab_manager: &mut TabManager,
        plugins: &PluginManager,
        theme_bg: (u8, u8, u8),
        _tabs_enabled: bool,
    ) -> Vec<FileAction> {
        let text = {
            if let Some(doc) = tab_manager.active_doc() {
                buffer_text_no_leak(&doc.buffer)
            } else {
                return vec![];
            }
        };

        if let Some(path) = native_save_dialog(self.last_open_directory.as_deref(), theme_bg) {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                self.last_open_directory = Some(parent.to_string_lossy().to_string());
            }
            match fs::write(&path, &text) {
                Ok(_) => {
                    let id = {
                        if let Some(doc) = tab_manager.active_doc_mut() {
                            doc.file_path = Some(path.clone());
                            doc.update_display_name();
                            doc.mark_clean();
                            doc.disk_mtime =
                                fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                            Some(doc.id)
                        } else {
                            None
                        }
                    };

                    let mut actions = Vec::new();
                    if let Some(id) = id {
                        actions.push(FileAction::DetectAndHighlight(id, path.clone()));
                        actions.push(FileAction::BindHighlightData(id));
                        actions.push(FileAction::UpdatePreviewFile {
                            doc_id: id.0,
                            path: path.clone(),
                            text: text.clone(),
                        });
                    }
                    actions.push(FileAction::UpdateWindowTitle);
                    actions.push(FileAction::RebuildTabBar);
                    actions.push(FileAction::UpdateMenusForFileType);

                    // Call lint hook after successful save
                    let lint_result = plugins.call_hook(PluginHook::OnDocumentLint {
                        path,
                        content: text,
                    });
                    actions.push(FileAction::ProcessLintResult(Box::new(lint_result)));

                    actions
                }
                Err(e) => {
                    vec![FileAction::ShowError(format!("Error saving file: {}", e))]
                }
            }
        } else {
            vec![]
        }
    }

    /// Reload a single document from disk.
    pub fn reload_file(&self, doc_id: DocumentId, tab_manager: &mut TabManager) -> Vec<FileAction> {
        let (path, is_partial) = {
            if let Some(doc) = tab_manager.doc_by_id(doc_id) {
                let path = match doc.file_path.as_ref() {
                    Some(p) => p.clone(),
                    None => return vec![], // untitled
                };
                let is_partial =
                    doc.display_name.contains("(tail)") || doc.display_name.contains("(lines ");
                (path, is_partial)
            } else {
                return vec![];
            }
        };

        if is_partial {
            return vec![];
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                return vec![FileAction::ShowError(format!(
                    "Error reloading file: {}",
                    e
                ))];
            }
        };
        let new_mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());

        if let Some(doc) = tab_manager.doc_by_id_mut(doc_id) {
            doc.buffer.set_text(&content);
            let default_style = "A".repeat(content.len());
            doc.style_buffer.set_text(&default_style);
            doc.has_unsaved_changes.set(false);
            doc.checkpoints.clear();
            doc.disk_mtime = new_mtime;
            doc.diagnostics.clear();
            doc.has_been_linted = false;
            doc.cached_tree = None;
            doc.cached_line_count = content.lines().count();
        }

        let mut actions = vec![
            FileAction::DetectAndHighlight(doc_id, path.clone()),
            FileAction::RebuildTabBar,
            FileAction::UpdateWindowTitle,
        ];

        if content.len() > DEFERRED_THRESHOLD {
            actions.push(FileAction::DeferOpenHooks { path, content });
        } else {
            actions.push(FileAction::RunOpenHooks { path, content });
        }
        actions
    }

    /// Reload all file-backed documents from disk.
    pub fn reload_all_files(&self, tab_manager: &mut TabManager) -> Vec<FileAction> {
        let ids: Vec<DocumentId> = tab_manager
            .documents()
            .iter()
            .filter(|d| d.file_path.is_some())
            .map(|d| d.id)
            .collect();

        let mut all_actions = Vec::new();
        for id in ids {
            all_actions.extend(self.reload_file(id, tab_manager));
        }
        all_actions
    }

    /// Check which open documents have been modified or deleted externally.
    pub fn check_external_modifications(
        tab_manager: &TabManager,
    ) -> Vec<(DocumentId, String, bool, ExternalChange)> {
        let mut modified = Vec::new();
        for doc in tab_manager.documents() {
            let path = match doc.file_path.as_ref() {
                Some(p) => p,
                None => continue,
            };
            let stored_mtime = match doc.disk_mtime {
                Some(m) => m,
                None => continue,
            };
            match fs::metadata(path).and_then(|m| m.modified()) {
                Ok(current_mtime) if current_mtime != stored_mtime => {
                    modified.push((
                        doc.id,
                        path.clone(),
                        doc.is_dirty(),
                        ExternalChange::Modified,
                    ));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    modified.push((
                        doc.id,
                        path.clone(),
                        doc.is_dirty(),
                        ExternalChange::Deleted,
                    ));
                }
                Err(_) => {
                    // Other I/O error (permissions, etc.) — treat as modified
                    modified.push((
                        doc.id,
                        path.clone(),
                        doc.is_dirty(),
                        ExternalChange::Modified,
                    ));
                }
                _ => {} // unchanged
            }
        }
        modified
    }

    /// Update the preview HTML file if the saved file is markdown.
    pub fn update_preview_file(
        preview: &mut PreviewController,
        dark_mode: bool,
        doc_id: u64,
        path: &str,
        text: &str,
    ) {
        if !PreviewController::is_markdown_file(Some(path)) {
            return;
        }

        let raw_html = PreviewController::render_markdown(text);
        let base_dir = std::path::Path::new(path).parent();
        let wrapped = wrap_html_for_preview(&raw_html, dark_mode, base_dir);

        let _ = preview.write_html(Some(path), doc_id, &wrapped);
    }

    // ===== Internal helpers =====

    /// Open file content that has already been read.
    fn open_file_content(
        &self,
        path: String,
        content: String,
        tab_manager: &mut TabManager,
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        if tabs_enabled {
            if let Some(existing_id) = tab_manager.find_by_path(&path) {
                return vec![
                    FileAction::SwitchToDocument(existing_id),
                    FileAction::RebuildTabBar,
                ];
            }
            // Close empty Untitled tab if it's the only one
            let empty_untitled = if tab_manager.count() == 1 {
                tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none() && !doc.is_dirty() && doc.buffer.length() == 0 {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let id = tab_manager.add_from_file(path.clone(), &content);
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.disk_mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            }
            if let Some(untitled_id) = empty_untitled {
                tab_manager.remove(untitled_id);
            }

            let mut actions = vec![
                FileAction::DetectAndHighlight(id, path.clone()),
                FileAction::SwitchToDocument(id),
                FileAction::RebuildTabBar,
            ];

            if content.len() > DEFERRED_THRESHOLD {
                actions.push(FileAction::DeferOpenHooks { path, content });
            } else {
                actions.push(FileAction::RunOpenHooks { path, content });
            }
            actions
        } else {
            if let Some(doc) = tab_manager.active_doc_mut() {
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.update_display_name();
                doc.disk_mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            }
            let mut actions = Vec::new();
            if let Some(id) = tab_manager.active_id() {
                actions.push(FileAction::DetectAndHighlight(id, path.clone()));
            }
            actions.push(FileAction::UpdateWindowTitle);
            actions.push(FileAction::UpdateMenusForFileType);

            if content.len() > DEFERRED_THRESHOLD {
                actions.push(FileAction::DeferOpenHooks { path, content });
            } else {
                actions.push(FileAction::RunOpenHooks { path, content });
            }
            actions
        }
    }

    /// Open large file from a pre-populated TextBuffer (memory-optimized).
    /// Skips plugins and syntax highlighting.
    fn open_large_file_buffer(
        &self,
        path: String,
        buffer: fltk::text::TextBuffer,
        tab_manager: &mut TabManager,
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        if tabs_enabled {
            if let Some(existing_id) = tab_manager.find_by_path(&path) {
                return vec![
                    FileAction::SwitchToDocument(existing_id),
                    FileAction::RebuildTabBar,
                ];
            }
            // Close empty Untitled tab if it's the only one
            let empty_untitled = if tab_manager.count() == 1 {
                tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none() && !doc.is_dirty() && doc.buffer.length() == 0 {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let id = tab_manager.add_from_buffer(path.clone(), buffer, true);
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.disk_mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            }
            if let Some(untitled_id) = empty_untitled {
                tab_manager.remove(untitled_id);
            }
            vec![FileAction::SwitchToDocument(id), FileAction::RebuildTabBar]
        } else {
            if let Some(doc) = tab_manager.active_doc_mut() {
                let content = crate::app::infrastructure::buffer::buffer_text_no_leak(&buffer);
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.disk_mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                doc.file_path = Some(path);
                doc.update_display_name();
            }
            vec![FileAction::UpdateWindowTitle]
        }
    }

    /// Open content from a file tail (last N lines).
    fn open_tail_content(
        &self,
        path: String,
        content: String,
        filename: &str,
        start_byte: u64,
        tab_manager: &mut TabManager,
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        use crate::app::domain::document::PartialFileInfo;

        if tabs_enabled {
            let empty_untitled = if tab_manager.count() == 1 {
                tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none() && !doc.is_dirty() && doc.buffer.length() == 0 {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            let id = tab_manager.add_from_file(path.clone(), &content);
            if let Some(untitled_id) = empty_untitled {
                tab_manager.remove(untitled_id);
            }

            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.display_name = format!("{} (tail)", filename);
                doc.partial_info = PartialFileInfo::Tail { start_byte };
                doc.has_unsaved_changes.set(false);
            }

            vec![
                FileAction::SwitchToDocument(id),
                FileAction::RebuildTabBar,
                FileAction::DeferOpenHooks { path, content },
            ]
        } else {
            if let Some(doc) = tab_manager.active_doc_mut() {
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.display_name = format!("{} (tail)", filename);
                doc.partial_info = PartialFileInfo::Tail { start_byte };
            }
            vec![
                FileAction::UpdateWindowTitle,
                FileAction::DeferOpenHooks { path, content },
            ]
        }
    }

    /// Open content from a specific line range (chunk).
    #[allow(clippy::too_many_arguments)]
    fn open_chunk_content(
        &self,
        path: String,
        content: String,
        filename: &str,
        start_line: usize,
        end_line: usize,
        start_byte: u64,
        end_byte: u64,
        tab_manager: &mut TabManager,
        tabs_enabled: bool,
    ) -> Vec<FileAction> {
        use crate::app::domain::document::PartialFileInfo;

        let chunk_label = format!("{} (lines {}-{})", filename, start_line, end_line);

        if tabs_enabled {
            let empty_untitled = if tab_manager.count() == 1 {
                tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none() && !doc.is_dirty() && doc.buffer.length() == 0 {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            let id = tab_manager.add_from_file(path.clone(), &content);
            if let Some(untitled_id) = empty_untitled {
                tab_manager.remove(untitled_id);
            }

            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.display_name = chunk_label;
                doc.partial_info = PartialFileInfo::Chunk {
                    start_byte,
                    end_byte,
                };
                doc.has_unsaved_changes.set(false);
            }

            vec![
                FileAction::SwitchToDocument(id),
                FileAction::RebuildTabBar,
                FileAction::DeferOpenHooks { path, content },
            ]
        } else {
            if let Some(doc) = tab_manager.active_doc_mut() {
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.display_name = chunk_label;
                doc.partial_info = PartialFileInfo::Chunk {
                    start_byte,
                    end_byte,
                };
            }
            vec![
                FileAction::UpdateWindowTitle,
                FileAction::DeferOpenHooks { path, content },
            ]
        }
    }
}

/// What a path given on the command line resolves to.
#[derive(Debug, PartialEq, Eq)]
pub enum CliOpenTarget {
    /// An existing regular file: open it as usual.
    Open(String),
    /// The file does not exist yet but its folder does: start a new, empty
    /// document bound to this path (the parent is canonicalized).
    NewFile(String),
    /// Refuse the path; the string is the message shown to the user.
    Invalid(String),
}

/// Decide how to open a path that came from the command line.
///
/// The path is untrusted input, so this fails closed: an existing path must
/// be a regular file (symlinks are followed, dangling ones are refused); a
/// missing path is only accepted when its parent canonicalizes to an existing
/// directory. Nothing is created on disk here, and directories never are.
pub fn resolve_cli_open_target(path: &str) -> CliOpenTarget {
    let path_ref = std::path::Path::new(path);
    match fs::symlink_metadata(path_ref) {
        Ok(_) => {
            // Follow symlinks: the target must be a regular file.
            return match fs::metadata(path_ref) {
                Ok(meta) if meta.is_file() => CliOpenTarget::Open(path.to_string()),
                _ => CliOpenTarget::Invalid(format!(
                    "Cannot open '{}': it is not a regular file.",
                    path
                )),
            };
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return CliOpenTarget::Invalid(format!("Cannot open '{}': {}", path, e)),
    }

    // `file_name()` is None for paths ending in `..` or a root.
    let Some(file_name) = path_ref.file_name() else {
        return CliOpenTarget::Invalid(format!("Cannot open '{}': not a file path.", path));
    };
    let parent = match path_ref.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let canonical_parent = match fs::canonicalize(parent) {
        Ok(p) if p.is_dir() => p,
        _ => {
            return CliOpenTarget::Invalid(format!(
                "Cannot create '{}': the folder '{}' does not exist.",
                path,
                parent.display()
            ));
        }
    };

    let new_path = strip_verbatim_prefix(canonical_parent.join(file_name));
    // Re-check after canonicalization: the resolved path must still be free.
    if fs::symlink_metadata(&new_path).is_ok() {
        return CliOpenTarget::Invalid(format!(
            "Cannot create '{}': the path already exists.",
            path
        ));
    }
    match new_path.to_str() {
        Some(s) => CliOpenTarget::NewFile(s.to_string()),
        None => CliOpenTarget::Invalid(format!("Cannot create '{}': the path is not UTF-8.", path)),
    }
}

/// `fs::canonicalize` returns `\\?\C:\...` on Windows; keep the plain drive
/// form so titles and saved sessions show the usual path.
#[cfg(windows)]
fn strip_verbatim_prefix(p: std::path::PathBuf) -> std::path::PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => std::path::PathBuf::from(rest),
        _ => p,
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(p: std::path::PathBuf) -> std::path::PathBuf {
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_str(p: &std::path::Path) -> String {
        p.to_str().expect("temp path is UTF-8").to_string()
    }

    #[test]
    fn open_missing_path_with_existing_parent_starts_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("new.txt");
        let expected = std::fs::canonicalize(dir.path()).unwrap().join("new.txt");

        assert_eq!(
            resolve_cli_open_target(&path_str(&target)),
            CliOpenTarget::NewFile(path_str(&expected))
        );
        assert!(!target.exists(), "resolving must not create the file");
    }

    #[test]
    fn open_missing_path_with_missing_parent_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing_dir = dir.path().join("nodir");
        let target = missing_dir.join("new.txt");

        assert!(matches!(
            resolve_cli_open_target(&path_str(&target)),
            CliOpenTarget::Invalid(_)
        ));
        assert!(!missing_dir.exists(), "no directory may be created");
    }

    #[test]
    fn open_directory_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();

        assert!(matches!(
            resolve_cli_open_target(&path_str(dir.path())),
            CliOpenTarget::Invalid(_)
        ));
    }

    #[test]
    fn open_existing_file_keeps_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();

        assert_eq!(
            resolve_cli_open_target(&path_str(&file)),
            CliOpenTarget::Open(path_str(&file))
        );
    }

    #[test]
    fn open_missing_path_resolves_dot_dot_in_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let target = dir.path().join("sub").join("..").join("new.txt");
        let expected = std::fs::canonicalize(dir.path()).unwrap().join("new.txt");

        assert_eq!(
            resolve_cli_open_target(&path_str(&target)),
            CliOpenTarget::NewFile(path_str(&expected))
        );
    }

    #[test]
    fn open_missing_path_under_a_missing_dir_with_dot_dot_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nodir").join("..").join("new.txt");

        assert!(matches!(
            resolve_cli_open_target(&path_str(&target)),
            CliOpenTarget::Invalid(_)
        ));
    }

    #[test]
    fn open_path_ending_in_dot_dot_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nodir").join("..");

        assert!(matches!(
            resolve_cli_open_target(&path_str(&target)),
            CliOpenTarget::Invalid(_)
        ));
    }

    #[test]
    fn open_missing_path_whose_parent_is_a_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();

        assert!(matches!(
            resolve_cli_open_target(&path_str(&file.join("new.txt"))),
            CliOpenTarget::Invalid(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn open_dangling_symlink_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(dir.path().join("elsewhere.txt"), &link).unwrap();

        assert!(matches!(
            resolve_cli_open_target(&path_str(&link)),
            CliOpenTarget::Invalid(_)
        ));
        assert!(!dir.path().join("elsewhere.txt").exists());
    }
}

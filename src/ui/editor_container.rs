use fltk::{
    enums::Color,
    group::{Flex, Tile},
    misc::HelpView,
    prelude::*,
    text::{TextBuffer, TextEditor},
};

pub struct EditorContainer {
    parent_flex: Flex,
    editor: TextEditor,
    /// Created once on first show_preview(), then reused across hide/show cycles.
    help_view: Option<HelpView>,
    tile: Option<Tile>,
    is_split: bool,
}

impl EditorContainer {
    /// Create an EditorContainer. The TextEditor is added directly to `parent_flex`
    /// as its fill child (no wrapper group).
    pub fn new(parent_flex: &Flex) -> Self {
        let mut editor = TextEditor::new(0, 0, 0, 0, "");
        editor.set_buffer(TextBuffer::default());
        editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
        editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));

        Self {
            parent_flex: parent_flex.clone(),
            editor,
            help_view: None,
            tile: None,
            is_split: false,
        }
    }

    /// Show the preview split. Creates a Tile wrapper each time (FLTK requires
    /// fresh parent-child relationships), but reuses the HelpView widget.
    pub fn show_preview(&mut self) {
        if self.is_split {
            return;
        }

        let fx = self.parent_flex.x();
        let fy = self.parent_flex.y();
        let fw = self.parent_flex.w();
        let fh = self.parent_flex.h();
        let half_w = fw / 2;

        // Remove the bare editor from the parent flex
        self.parent_flex.remove(&self.editor);

        // Create a fresh Tile (FLTK widgets can only have one parent,
        // so we can't reuse a detached Tile after reparenting the editor)
        let mut tile = Tile::new(fx, fy, fw, fh, None);

        self.editor.resize(fx, fy, half_w, fh);
        tile.add(&self.editor);

        // Reuse existing HelpView or create one on first call
        if let Some(ref mut hv) = self.help_view {
            hv.resize(fx + half_w, fy, fw - half_w, fh);
            hv.show();
            tile.add(hv);
        } else {
            let mut hv = HelpView::new(fx + half_w, fy, fw - half_w, fh, None);
            hv.set_text_size(14);
            tile.add(&hv);
            self.help_view = Some(hv);
        }

        tile.end();
        self.parent_flex.add(&tile);
        self.tile = Some(tile);

        self.is_split = true;
        self.parent_flex.layout();
        self.parent_flex.redraw();
    }

    /// Hide the preview, restoring the editor as a direct child of the parent Flex.
    /// The HelpView is kept alive for reuse (avoids re-allocating the heavy widget).
    pub fn hide_preview(&mut self) {
        if !self.is_split {
            return;
        }

        // Clear HelpView content so FLTK releases Fl_Shared_Image references
        if let Some(ref mut hv) = self.help_view {
            hv.set_value("");
            hv.hide();
        }

        // Properly destroy the Tile C++ widget.
        // CRITICAL: Remove children FIRST — Fl_Group::~Fl_Group() calls clear()
        // which deletes all children.  We must not let it delete our editor/help_view.
        if let Some(mut tile) = self.tile.take() {
            tile.remove(&self.editor);
            if let Some(ref hv) = self.help_view {
                tile.remove(hv);
            }
            self.parent_flex.remove(&tile);
            fltk::app::delete_widget(tile);
        }

        // Re-add the bare editor to the parent flex
        self.parent_flex.add(&self.editor);

        self.is_split = false;
        self.parent_flex.layout();
        self.parent_flex.redraw();
    }

    /// Load an HTML file into the HelpView.
    /// Using load() instead of set_value() so HelpView sets its base directory
    /// and can resolve relative image paths.
    pub fn load_preview_file(&mut self, path: &str) {
        if let Some(ref mut hv) = self.help_view {
            let _ = hv.load(path);
        }
    }

    /// Set HTML directly (fallback when no file path available).
    pub fn load_preview_fallback(&mut self, html: &str) {
        if let Some(ref mut hv) = self.help_view {
            hv.set_value(html);
        }
    }

    pub fn editor(&self) -> &TextEditor {
        &self.editor
    }

    pub fn is_split(&self) -> bool {
        self.is_split
    }

    /// Number of direct children in the parent Flex (for debug leak detection).
    #[cfg(debug_assertions)]
    pub fn parent_flex_children(&self) -> i32 {
        self.parent_flex.children()
    }
}

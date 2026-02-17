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

    /// Show the preview split. Replaces the editor in the parent Flex with a Tile.
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

        // Create a Tile with editor (left) + HelpView (right)
        let mut tile = Tile::new(fx, fy, fw, fh, None);

        self.editor.resize(fx, fy, half_w, fh);
        tile.add(&self.editor);

        let mut hv = HelpView::new(fx + half_w, fy, fw - half_w, fh, None);
        hv.set_text_size(14);
        tile.add(&hv);

        tile.end();

        // Add the Tile to the parent flex (takes the fill slot)
        self.parent_flex.add(&tile);

        self.help_view = Some(hv);
        self.tile = Some(tile);
        self.is_split = true;

        self.parent_flex.layout();
        self.parent_flex.redraw();
    }

    /// Hide the preview, restoring the editor as a direct child of the parent Flex.
    pub fn hide_preview(&mut self) {
        if !self.is_split {
            return;
        }

        // Remove the Tile from the parent flex
        if let Some(ref tile) = self.tile {
            self.parent_flex.remove(tile);
        }

        // Re-add the bare editor to the parent flex
        self.parent_flex.add(&self.editor);

        self.help_view = None;
        self.tile = None;
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
}

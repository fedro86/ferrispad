use fltk::{
    enums::Color,
    group::Flex,
    prelude::*,
    text::{TextBuffer, TextEditor},
};

pub struct EditorContainer {
    editor: TextEditor,
}

impl EditorContainer {
    pub fn new(_parent_flex: &Flex) -> Self {
        let mut editor = TextEditor::new(0, 0, 0, 0, "");
        editor.set_buffer(TextBuffer::default());

        // Enable native line numbers
        editor.set_linenumber_width(40);
        editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
        editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));

        Self { editor }
    }

    pub fn editor(&self) -> &TextEditor {
        &self.editor
    }
}

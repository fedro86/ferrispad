use fltk::enums::Font;

use super::document::DocumentId;
use super::updater::ReleaseInfo;

/// All messages that can be sent through the FLTK channel.
/// Each menu callback sends one of these; the dispatch loop in main handles them.
#[derive(Debug, Clone)]
pub enum Message {
    // File
    FileNew,
    FileOpen,
    FileSave,
    FileSaveAs,
    FileQuit,
    WindowClose,

    // Tabs
    TabSwitch(DocumentId),
    TabClose(DocumentId),
    TabCloseActive,
    TabMove(usize, usize),
    TabNext,
    TabPrevious,

    // Edit
    EditUndo,
    EditRedo,
    EditCut,
    EditCopy,
    EditPaste,
    SelectAll,
    ShowFind,
    ShowReplace,
    ShowGoToLine,

    // View
    ToggleLineNumbers,
    ToggleWordWrap,
    ToggleDarkMode,
    ToggleHighlighting,

    // Format
    SetFont(Font),
    SetFontSize(i32),

    // Settings & Help
    OpenSettings,
    CheckForUpdates,
    ShowAbout,

    // Syntax highlighting
    BufferModified(DocumentId, i32),
    DoRehighlight,
    ContinueHighlight,

    // Background updates
    BackgroundUpdateResult(Option<ReleaseInfo>),
    ShowBannerUpdate,
    DismissBanner,
}

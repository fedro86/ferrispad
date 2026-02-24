use fltk::enums::Font;

use super::document::DocumentId;
use super::settings::SyntaxTheme;
use crate::app::controllers::tabs::{GroupColor, GroupId};
use crate::app::plugins::{Diagnostic, LineAnnotation};
use crate::app::services::updater::ReleaseInfo;
use crate::ui::toast::ToastLevel;

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

    // Tab Groups
    TabGroupCreate(DocumentId),
    TabGroupDelete(GroupId),
    TabGroupClose(GroupId),
    TabGroupRename(GroupId),
    TabGroupRecolor(GroupId, GroupColor),
    TabGroupAddTab(DocumentId, GroupId),
    TabGroupRemoveTab(DocumentId),
    TabGroupToggle(GroupId),
    TabGroupByDrag(DocumentId, DocumentId),
    TabGroupMove(GroupId, usize),

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
    TogglePreview,

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

    // Live preview from settings dialog
    PreviewSyntaxTheme(SyntaxTheme),

    // Plugin system
    PluginsToggleGlobal,
    PluginToggle(String),
    PluginsReloadAll,
    CheckPluginPermissions,
    /// A plugin's custom menu action was triggered
    PluginMenuAction { plugin_name: String, action: String },

    // Diagnostics
    DiagnosticsUpdate(Vec<Diagnostic>),
    #[allow(dead_code)]  // Reserved for explicit clear from UI
    DiagnosticsClear,
    DiagnosticGoto(u32),  // Go to line number (single click)
    DiagnosticOpenDocs(u32),  // Open documentation URL (double click)

    // Line annotations (gutter + inline highlights)
    #[allow(dead_code)]  // Reserved for future batch annotation updates
    AnnotationsUpdate(Vec<LineAnnotation>),
    #[allow(dead_code)]  // Reserved for explicit clear from UI
    AnnotationsClear,
    ManualHighlight,  // Triggered by Ctrl+Shift+L

    // Toast notifications
    ToastShow(ToastLevel, String),
    ToastHide,
}

use fltk::enums::Font;

use super::document::DocumentId;
use super::settings::SyntaxTheme;
use crate::app::controllers::tabs::{GroupColor, GroupId};
use crate::app::plugins::{Diagnostic, LineAnnotation, SplitViewRequest, TreeViewRequest};
use crate::app::services::plugin_update_checker::PluginUpdateInfo;
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
    /// Move a tab to a position and optionally change its group atomically
    TabMoveToGroup(DocumentId, usize, Option<GroupId>),

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
    ShowKeyShortcuts,

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
    /// Open the plugin manager dialog
    ShowPluginManager,
    /// Open the plugin settings dialog (Run All Checks config)
    ShowPluginSettings,
    /// Open per-plugin configuration dialog
    ShowPluginConfig(String),
    /// Check for plugin updates (triggers background check)
    CheckPluginUpdates,
    /// Plugin update check completed with results
    PluginUpdatesChecked(Vec<PluginUpdateInfo>),

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

    // Window events
    WindowResize,

    // Widget API - Split View
    /// Show a split view requested by a plugin
    SplitViewShow {
        session_id: u32,
        plugin_name: String,
        request: SplitViewRequest,
    },
    /// Hide the current split view
    SplitViewHide(u32),
    /// User clicked Accept in split view
    SplitViewAccept(u32),
    /// User clicked Reject in split view
    SplitViewReject(u32),

    // Widget API - Tree View
    /// Show a tree view requested by a plugin
    TreeViewShow {
        session_id: u32,
        plugin_name: String,
        request: TreeViewRequest,
    },
    /// Hide the current tree view
    TreeViewHide(u32),
    /// User clicked a node in the tree view
    TreeViewNodeClicked {
        session_id: u32,
        node_path: Vec<String>,
    },
}

use super::document::DocumentId;
use super::settings::SyntaxTheme;
use crate::app::controllers::tabs::{GroupColor, GroupId};
use crate::app::plugins::{
    Diagnostic, LineAnnotation, SplitViewRequest, TerminalViewRequest, TreeViewRequest,
};
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
    FileReload,
    FileReloadAll,
    FileQuit,
    WindowClose,
    WindowFocusGained,

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
    ToggleDiagnosticsPanel,

    // Format
    SetFont(String),
    SetFontSize(i32),
    OpenFontPicker,

    // Settings & Help
    OpenSettings,
    CheckForUpdates,
    ShowAbout,
    ShowKeyShortcuts,

    // Syntax highlighting
    BufferModified {
        id: DocumentId,
        pos: i32,
        inserted: i32,
        deleted: i32,
    },
    DoRehighlight,
    ContinueHighlight,
    /// Debounced text change hook (fires 300ms after last edit)
    DoTextChangeHook,

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
    PluginMenuAction {
        plugin_name: String,
        action: String,
    },
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
    DiagnosticsClear,
    DiagnosticGoto(u32),     // Go to line number (single click)
    DiagnosticOpenDocs(u32), // Open documentation URL (double click)
    DiagnosticsAutoDismiss,  // Auto-dismiss "All checks passed" green bar after timeout

    // Line annotations (gutter + inline highlights)
    AnnotationsUpdate(Vec<LineAnnotation>),
    AnnotationsClear,
    ManualHighlight, // Triggered by Ctrl+Shift+L

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
    /// User expanded a lazy-load node in the tree view
    TreeViewNodeExpanded {
        session_id: u32,
        node_path: Vec<String>,
    },
    /// User triggered a context menu action on a tree node
    TreeViewContextAction {
        session_id: u32,
        action: String,
        node_path: Vec<String>,
        input_text: Option<String>,
        target_path: Option<Vec<String>>,
    },
    /// User typed in the tree view search bar
    TreeViewSearch {
        query: String,
    },

    /// Deferred plugin hooks for large files (run after event loop processes banner)
    DeferredPluginHooks {
        path: String,
        content: String,
    },
    /// Deferred tree view refresh on tab switch (avoids blocking UI for large files)
    DeferredTreeRefresh {
        path: Option<String>,
        content: String,
    },
    /// Show "Loading..." placeholder in tree panel (keeps panel visible during refresh)
    TreeViewLoading,
    /// Deferred session restore (runs after window is shown so UI is visible immediately)
    DeferredSessionRestore,
    /// Open a file from CLI args (deferred so session restore runs first)
    DeferredOpenFile(String),
    /// Jump to a line number from CLI --line flag (deferred after file opens)
    DeferredGotoLine(usize),
    /// User dragged the tree panel divider to resize
    TreeViewResize(i32),

    /// User dragged the split panel divider to resize
    SplitViewResize(i32),

    // Widget API - Terminal View
    /// Show a terminal view requested by a plugin
    TerminalViewShow {
        session_id: u32,
        plugin_name: String,
        request: TerminalViewRequest,
    },
    /// Hide the current terminal view
    TerminalViewHide(u32),
    /// Terminal produced output (signal to drain shared buffer)
    TerminalOutput(Vec<u8>),
    /// Terminal child process exited
    TerminalExited,
    /// User dragged the terminal panel divider to resize
    TerminalViewResize(i32),

    /// Deferred malloc_trim to return freed C++ pages to the OS without blocking UI
    MallocTrim,

    /// MCP request from the TCP server thread
    McpRequest {
        request_id: u64,
        json_rpc_id: serde_json::Value,
        method: String,
        params: serde_json::Value,
    },

    /// User clicked the diff tab in the tab bar
    DiffTabActivate(u32),
    /// Toggle split view between panel and tab display mode
    SplitViewToggleMode(u32),

    // Sessions
    /// Show the session picker dialog
    SessionShowPicker,
    /// Switch to a named session (save current, load target)
    SessionSwitchTo(String),
    /// Open a named session in a new window (spawns new process)
    SessionOpenInNewWindow(String),
    /// Save current session under a new name and switch to it
    SessionSaveAs(String),
    /// Delete a named session
    SessionDelete(String),
    /// Prompt for a name and open a new empty session in a new window
    SessionNewWindow,
}

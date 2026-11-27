local M = { name = "tree-demo", version = "0.1.0", description = "screenshot fixture" }

local function tree()
  return {
    title = "demo",
    persistent = true,
    expand_depth = 2,
    root = { label = "project", expanded = true, children = {
      { label = "modified.rs", label_color = "modified" },
      { label = "added.rs", label_color = "added" },
      { label = "conflict.rs", label_color = "conflict" },
      { label = "ignored.log", label_color = "ignored" },
      { label = "plain.txt" },
    } },
  }
end

local function diags()
  return {
    { line = 1, column = 4, message = "an error example", level = "error" },
    { line = 2, column = 9, message = "a warning example", level = "warning" },
    { line = 3, column = 9, message = "an info example", level = "info" },
    { line = 4, column = 5, message = "a hint example", level = "hint" },
  }
end

function M.on_document_open(api, path)
  return { tree_view = tree(), diagnostics = diags() }
end

function M.on_document_lint(api, path, content)
  return { diagnostics = diags() }
end

function M.on_menu_action(api, action, path, content)
  return { tree_view = tree(), diagnostics = diags() }
end

return M

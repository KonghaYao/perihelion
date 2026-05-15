# ============================================================
# Peri TUI — English (en) Translation File
# This is the fallback language; keys missing in other
# languages resolve to the values defined here.
# ============================================================

# ---- i18n infrastructure test keys ----
test-hello = Hello, World!
test-greeting = Hello, { $name }!
ui-empty = (none)

# ---- Command Descriptions ----

command-help-description = List all available commands
command-clear-description = Clear message list
command-compact-description = Compact conversation context (structured summary + re-inject recent files/Skills)
command-model-description = Open model selection panel (Provider + Level + Thinking); with args, switch alias directly (opus/sonnet/haiku)
command-login-description = Manage Provider configuration (create/edit/delete)
command-cost-description = View current session cost and token usage
command-context-description = View context usage and session statistics
command-agents-description = Open Agent selection panel
command-mcp-description = Manage MCP server connections
command-memory-description = Edit user/project-level CLAUDE.md memory files
command-history-description = Open conversation history browser
command-loop-description = Register scheduled loop task (natural language description, e.g. /loop remind me to drink water every 5 minutes)
command-cron-description = View and manage scheduled tasks
command-plugin-description = Manage plugins (browse, install, uninstall)
command-config-description = Global configuration (autocompact, language, system prompt overrides)
command-hooks-description = View Hook configuration
command-effort-description = View or set reasoning effort (low/medium/high/xhigh/max)
command-doctor-description = Diagnose configuration completeness
command-rename-description = View or modify current session title
command-split-description = Create a new split session
command-lang-description = Switch interface language (e.g. /lang zh-CN)
command-agent-description = Set Agent definition, switch different Agent roles

# ---- Command Execution Messages ----

# help command
help-available-commands = Available commands:
help-alias-prefix = (aliases: /{ $aliases })
help-skills-count = Skills ({ $count } available): type # prefix to view
help-skills-empty = Skills: place .md files in .claude/skills/ directory to add
help-shortcuts = Shortcuts: Shift+Tab toggle permission mode | Alt+M switch model | Esc quit | Ctrl+C interrupt

# compact command
compact-agent-running = Agent is running, cannot compact

# history command
history-agent-running = Agent is running, cannot open history panel

# model command
config-save-failed = Configuration save failed: { $error }

# effort command
effort-set = Reasoning effort set to { $effort }
effort-current = Current reasoning effort: { $effort }
effort-usage = Usage: /effort low|medium|high|xhigh|max

# loop command
loop-usage = Usage: /loop <natural language time description> <prompt>
loop-example = Example: /loop remind me to drink water every 5 minutes

# rename command
rename-no-session = No active session, cannot rename
rename-current-title = Current title: { $title }
rename-updated = Session title updated to: { $name }
rename-failed = Rename failed: { $error }
rename-untitled = (untitled)

# doctor command
doctor-header = Doctor check results:
doctor-settings-missing = Missing  Cannot get home directory
doctor-no-api-key = Missing  No API Key set (ANTHROPIC_API_KEY or OPENAI_API_KEY)
doctor-no-provider = No Provider  No provider configured
doctor-provider-not-found = No Provider  active_provider_id '{ $id }' not found
doctor-no-mcp = None  No MCP servers configured
doctor-mcp-not-init = None  .mcp.json exists but MCP not initialized
doctor-no-alias = No Alias  No model aliases configured
doctor-no-alias-no-provider = No Alias  No active Provider
doctor-no-alias-not-configured = No Alias  Not configured

# lang command
lang-switched = Language switched to { $lang }
lang-available = Available languages: { $langs }
lang-unsupported = Unsupported language: { $lang }

# ---- Status Bar ----

statusbar-permission-dont-ask = Don't Ask
statusbar-permission-accept-edit = Accept Edit
statusbar-permission-auto = Auto Mode
statusbar-permission-bypass = Bypass
statusbar-copied =  { $count } chars copied
statusbar-no-agent = None
statusbar-bg-indicator = [BG: { $count }]
statusbar-retrying = Retry { $attempt }/{ $max } ({ $delay }s): { $error }
statusbar-mcp-connecting =  MCP ({ $connected }/{ $total })...
statusbar-mcp-ready =  MCP ready ({ $total } servers)
statusbar-mcp-failed =  MCP failed: { $msg }
statusbar-lsp-diag = diag: { $errors }E/{ $warnings }W

# ---- Status Bar Shortcut Hints (main view) ----

key-command = command
key-switch-session = :Switch Session
key-close = :Close
key-scroll = :Scroll
key-cancel = :Cancel
key-newline = :NewLine
key-open-browser = :Open browser
key-submit = :Submit
key-switch = :Switch
key-move = :Move
key-select = :Select
key-confirm = :Confirm

# ---- Welcome Page ----

welcome-title = Peri Agent Framework
welcome-divider = ────── What can I do? ──────
welcome-feature-code = Ask me to code, debug, or refactor
welcome-feature-files = Manage files and run terminal commands
welcome-feature-agents = Delegate tasks to specialized sub-agents
welcome-login-hint-1 = Please type
welcome-login-hint-2 = to configure API Key to get started
welcome-shortcut-quit = :Quit
welcome-shortcut-stop = :Stop
welcome-shortcut-newline = :NewLine
welcome-shortcut-mode = :Mode
welcome-shortcut-model = :Model
welcome-skills-available = { $count } skills available

# ---- Tips (18 items) ----

tip-0 = Type / to enter commands, Tab to autocomplete
tip-1 = Ctrl+C interrupts Agent, Shift+Tab toggles permission mode
tip-2 = Alt+M quickly switch model (opus / sonnet / haiku)
tip-3 = Alt+Enter for newline in input box
tip-4 = Drag files or images to terminal to auto-attach to message
tip-5 = Long press Ctrl+V to paste clipboard image
tip-6 = Ctrl+U/D scroll message history, Up/Down browse input history
tip-7 = Ctrl+N/P switch Session, Ctrl+W close
tip-8 = Esc closes popup or panel, Enter confirms selection
tip-9 = /compact compresses context to save tokens
tip-10 = /clear clears current conversation
tip-11 = /model switches LLM model
tip-12 = /history browses conversation history
tip-13 = /loop creates scheduled loop tasks
tip-14 = /plugin manages Claude Code plugins
tip-15 = Add custom Skills in .claude/skills/
tip-16 = Define SubAgents in .claude/agents/
tip-17 = For complex tasks, have Agent plan first before executing

# ---- Setup Wizard ----

setup-welcome-title =  ── Peri Setup ── Welcome
setup-choose-provider =  Choose how to configure your provider:
setup-source-custom-api = Custom API
setup-source-migrate = Migrate from Claude Code
setup-source-custom-desc = Manually enter provider details
setup-source-migrate-desc = Import config from ~/.claude/
setup-key-confirm = :Confirm
setup-key-select = :Select
setup-key-quit = :Quit
setup-configure-title =  ── Peri Setup ── Configure Providers
setup-submit = Submit
setup-key-edit-submit = :Edit/Submit
setup-key-check = :Check
setup-key-back = :Back
setup-edit-title =  ── Setup ── Edit: { $type } ({ $id })
setup-field-type = Type
setup-field-id = ID
setup-field-base-url = Base URL
setup-field-api-key = API Key
setup-field-opus = Opus
setup-field-sonnet = Sonnet
setup-field-haiku = Haiku
setup-model-label = Model
setup-confirm = Confirm
setup-key-switch-type = :Switch type
setup-key-back-list = :Back to list
setup-complete-title =  ── Setup Complete ✓
setup-press-enter = Press
setup-to-start = to start using
setup-no-key = (no key)

# ---- Config Panel ----

config-panel-title-browse =  /config — Configuration
config-panel-title-edit =  /config — Edit Configuration
config-field-autocompact = Autocompact
config-field-compact-threshold = Compact Threshold
config-field-language = Language
config-field-persona = Persona
config-field-tone = Tone
config-field-proactiveness = Proactiveness
config-value-on = ON
config-value-off = OFF
config-saved = Configuration saved
config-key-navigate = :Navigate
config-key-edit = :Edit
config-key-save = :Save
config-key-toggle = :Toggle

# ---- Login Panel ----

login-panel-title-browse =  /login — Provider Management
login-panel-title-edit =  /login — Edit Provider
login-panel-title-new =  /login — New Provider
login-key-activate = :Activate
login-key-new = :New
login-key-delete = :Delete
login-key-paste = :Paste
login-confirm-delete = :Confirm delete

# ---- HITL Popup ----

hitl-single-title =  ⚠ Tool Approval (1 item)
hitl-batch-title =  ⚠ Batch Tool Approval
hitl-approved = [Approved]
hitl-rejected = [Rejected]
hitl-summary = Selected: { $approved } approved / { $rejected } rejected

# ---- AskUser Popup ----

ask-user-placeholder = Type something.

# ---- App Messages ----

app-provider-ready = { $name } ({ $model }) ready
app-not-configured = Not configured
app-empty = None
app-no-api-key-warning = Warning: No API Key set (ANTHROPIC_API_KEY or OPENAI_API_KEY)
app-interrupted-resumed = ⚠ Force interrupted (input restored to input box)
app-interrupt-done = ⚠ Interrupted
app-interrupted-background = ⚠ Force interrupted (background task may still be running)
app-config-saved = Configuration saved
app-config-save-failed = Configuration save failed: { $error }
app-provider-activated = Provider activated: { $name }
app-provider-created = Provider created and activated: { $name }
app-provider-saved = Provider saved and activated: { $name }
app-provider-deleted = Provider deleted: { $name }
app-provider-name-empty = Save failed: Provider name cannot be empty
app-agent-reset = Agent reset (no agent_id set)
app-agent-switched = Agent switched to: { $name } ({ $id })
app-agent-disconnected = Agent connection lost, please retry sending
app-compact-no-context = No compressible context (history is empty)
app-compact-no-provider = Compact failed: No LLM Provider configured (set ANTHROPIC_API_KEY or OPENAI_API_KEY)
app-compact-compressing = Compressing context
app-compact-done = Context compressed
app-compact-failed = Compact failed: { $error }
app-compact-auto-cleared = Auto cleanup: freed { $count } tool call results
app-compact-limit-reached = Context still exceeds limit after compression. Use /compact to manually compress or /clear to clear history.
app-model-switched = Model switched to: { $alias } ({ $effort } effort)
app-prompt-cache-low = Prompt cache hit rate { $rate }% < 80% (req: { $req })
app-no-mcp-configured = No MCP servers configured (add in .mcp.json or settings.json)
app-no-cron-tasks = No cron tasks
app-cron-deleted = Cron task deleted: { $preview }
app-submit-attachments = { $input } [{ $count } image(s)]
app-no-provider-submit = No API Key configured, type /login to configure Provider
app-bg-task-done = [Background task { $id } completed] Agent: { $agent } | Tool calls: { $tools } | Duration: { $duration }ms
app-bg-task-done-with-result = [Background task { $id } completed] Agent: { $agent } | Tool calls: { $tools } | Duration: { $duration }ms\nResult:\n{ $result }
app-bg-task-failed = [Background task { $id } failed] Agent: { $agent } | { $error }
app-bg-task-failed-with-error = [Background task { $id } failed] Agent: { $agent }\nError:\n{ $error }

# ---- Panel Status Bar Hints ----

# Login panel
hint-login-browse = :Navigate
hint-login-activate = :Activate
hint-login-edit = :Edit
hint-login-new = :New
hint-login-delete = :Delete
hint-login-close = :Close
hint-login-field = :Field
hint-login-save = :Save
hint-login-paste = :Paste
hint-login-toggle = :Toggle
hint-login-back = :Back

# Config panel
hint-config-browse-navigate = :Navigate
hint-config-browse-edit = :Edit
hint-config-edit-field = :Field
hint-config-edit-save = :Save

# Model panel
hint-model-navigate = :Navigate
hint-model-confirm = :Confirm
hint-model-effort = :Effort
hint-model-close = :Close

# Agent panel
hint-agent-select = :Select
hint-agent-confirm = :Confirm
hint-agent-cancel = :Cancel

# MCP panel
hint-mcp-navigate = :Navigate
hint-mcp-detail = :Detail
hint-mcp-reconnect = :Reconnect
hint-mcp-delete = :Delete
hint-mcp-execute = :Execute
hint-mcp-back = :Back
hint-mcp-close = :Close

# Plugin panel
hint-plugin-uninstall = :Confirm uninstall
hint-plugin-cancel = :Cancel
hint-plugin-delete = :Confirm delete
hint-plugin-add = :Add
hint-plugin-exit-search = :Exit search
hint-plugin-tab = :Tab
hint-plugin-install = :Install
hint-plugin-remove = :Remove
hint-plugin-navigate = :Navigate
hint-plugin-execute = :Execute
hint-plugin-back = :Back to list
hint-plugin-select = :Select
hint-plugin-search = :Search

# Cron panel
hint-cron-confirm-delete = :Confirm delete
hint-cron-navigate = :Navigate
hint-cron-toggle = :Toggle
hint-cron-delete = :Delete
hint-cron-close = :Close

# Status panel
hint-status-tab = :Switch Tab
hint-status-close = :Close

# History panel
hint-history-confirm-delete = :Confirm delete
hint-history-exit-search = :Exit search
hint-history-close = :Close

# Hooks panel
hint-hooks-navigate = :Navigate
hint-hooks-close = :Close

# Memory panel
hint-memory-select = :Select
hint-memory-edit = :Edit
hint-memory-close = :Close

# ---- Plugin Panel Messages ----

app-plugin-updating = Updating marketplace: { $name }
app-plugin-delete-failed = Delete failed: { $error }
app-plugin-add-failed = Add failed: { $error }
app-plugin-added = Marketplace added: { $name } (fetching content...)

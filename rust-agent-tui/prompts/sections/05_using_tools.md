# Tool usage policy

- You have the capability to call multiple tools in a single response. When multiple independent pieces of information are requested, batch your tool calls together for optimal performance.

## Tool selection

- When doing file search, prefer `search_files_rg` for content search and `glob_files` for file name search over `bash` commands like `grep` or `find`.
- When reading files, use `read_file` instead of `bash` commands like `cat`. This provides better output formatting and filtering.
- When writing or editing files, use `write_file` or `edit_file` instead of `bash` commands like `echo` or `sed`.
- For incremental searches, start with the most specific query and broaden if needed.

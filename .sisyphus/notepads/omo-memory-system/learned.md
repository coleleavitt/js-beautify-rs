# Learned

- OpenCode had static instruction loading, but not Claude-style persistent memory.
- OMO memory v1 uses file-based per-project isolation under ~/.config/opencode/oh-my-opencode/memory/<sanitized-cwd>/.
- One markdown file per memory with frontmatter: name, description, type.
- No selection model in v1; all memories for the project are injected at session start.
- Tools added: memory_save, memory_delete, memory_list.
- Session-start memory injection uses a system directive and avoids re-injecting within the same session.

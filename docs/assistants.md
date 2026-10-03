# Editing with AI assistants

**Updated:** 2026-10-03 (version 0.8.0)

OpenPremier speaks the [Model Context Protocol](https://modelcontextprotocol.io) (MCP), so an AI
assistant such as Claude Code, Codex, Cursor or any other MCP client can edit video with it:
import media, cut, move and speed up clips, add effects, transitions, titles and captions, look
at frames and export.

## Connecting

The program itself is the server: `OpenPremier --mcp` reads requests from standard input and
answers on standard output.

```bash
claude mcp add openpremier -- "C:\path\to\OpenPremier.exe" --mcp
```

```bash
codex mcp add openpremier -- "C:\path\to\OpenPremier.exe" --mcp
```

Other clients take the same command in their MCP settings, for example:

```json
{
  "mcpServers": {
    "openpremier": { "command": "C:\\path\\to\\OpenPremier.exe", "args": ["--mcp"] }
  }
}
```

Edit > Preferences > AI Assistants has buttons that copy these commands with the right path.

## Live editing or a project of its own

- With **Let AI assistants edit the open project** on (Preferences > AI Assistants), the tools act
  on the project open in the window and every edit appears as it happens. Each change is one undo
  step, so Edit > Undo takes back what the assistant did. Exports and transcriptions start in the
  window and show their progress there. The program listens only on 127.0.0.1, with a random key
  stored in its settings folder; the setting is off by default.
- Otherwise (the program closed, or the setting off) the assistant works on a project of its own,
  which it saves with `save_project`; `OpenPremier --mcp project.opproj` starts from a project.

## Tools

| Tool | What it does |
|---|---|
| `get_project`, `new_project`, `open_project`, `save_project` | The project: media, sequences, file |
| `import_media` | Imports files (video, audio and any picture format) and waits for them |
| `create_sequence`, `open_sequence` | Sequences, with a size and rate or matching some media |
| `get_timeline` | Tracks, clips (times in seconds), effects and their parameters, markers |
| `add_clip`, `delete_clips`, `move_clips`, `split_clips`, `set_speed` | Editing |
| `list_effects`, `add_effect`, `set_effect_params`, `remove_effect` | Effects, including Motion, Opacity and Volume |
| `add_transition` | A transition at a clip's start or end |
| `add_text` | A title |
| `transcribe_captions`, `set_caption_style` | Animated captions from the speech, and their look |
| `add_marker`, `set_playhead` | Markers and the playhead |
| `render_frame` | The picture at a time, so the assistant can see the result |
| `export` | A video file (H.264, HEVC, ProRes, DNxHR or PNG) |
| `run_command` | Any menu or keyboard command by its id |
| `undo`, `redo` | History |

Times are seconds of the sequence, tracks are named V1, V2 (video) and A1, A2 (audio), and
parameter values are numbers, true/false, text, `[x, y]` points from 0 to 1 or colors as
`[r, g, b]` from 0 to 1 or `"#rrggbb"`.

The protocol code is in `crates/op-mcp`; `tools.rs` lists the tools and runs them on an editor.

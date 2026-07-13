---
name: browser-use
description: Automates browser interactions for web testing, form filling, screenshots, and data extraction. Use when the user needs to navigate websites, interact with web pages, fill forms, take screenshots,
  or extract information from web pages.
description_zh: 浏览器自动化（导航、点击、截图、数据提取、多会话、云浏览器）
description_en: Automates browser interactions for web testing, form filling, screenshots, and data extraction
version: 2.0.4
allowed-tools: Bash(browser-use:*)
---

# Browser Automation with browser-use CLI

The `browser-use` command provides fast, persistent browser automation. A background daemon keeps the browser open across commands, giving ~50ms latency per call.

## Prerequisites

```bash
browser-use doctor    # Verify installation
```

For setup details, see https://github.com/browser-use/browser-use/blob/main/browser_use/skill_cli/README.md

## Core Workflow

1. **Navigate**: `browser-use open <url>` — launches headless browser and opens page
2. **Inspect**: `browser-use state` — returns clickable elements with indices
3. **Interact**: use indices from state (`browser-use click 5`, `browser-use input 3 "text"`)
4. **Verify**: `browser-use state` or `browser-use screenshot` to confirm
5. **Repeat**: browser stays open between commands

If a command fails, run `browser-use close` first to clear any broken session, then retry.

To use the user's existing Chrome (preserves logins/cookies): run `browser-use connect` first.
To use a cloud browser instead: run `browser-use cloud connect` first.
After either, commands work the same way.

### If `browser-use connect` fails

When `browser-use connect` cannot find a running Chrome with remote debugging, prompt the user with two options:

1. **Use their real Chrome browser** — they need to enable remote debugging first:
   - Open `chrome://inspect/#remote-debugging` in Chrome, or relaunch Chrome with `--remote-debugging-port=9222`
   - Then retry `browser-use connect`
2. **Use managed Chromium with their Chrome profile** — no Chrome setup needed:
   - Run `browser-use profile list` to show available profiles
   - Ask which profile they want, then use `browser-use --profile "ProfileName" open <url>`
   - This launches a separate Chromium instance with their profile data (cookies, logins, extensions)

Let the user choose — don't assume one path over the other.

## Browser Modes

```bash
browser-use open <url>                         # Default: headless Chromium (no setup needed)
browser-use --headed open <url>                # Visible window (for debugging)
browser-use connect                            # Connect to user's Chrome (preserves logins/cookies)
browser-use cloud connect                      # Cloud browser (zero-config, requires API key)
browser-use --profile "Default" open <url>     # Real Chrome with specific profile
```

After `connect` or `cloud connect`, all subsequent commands go to that browser — no extra flags needed.

## Commands

```bash
# Navigation
browser-use open <url>                    # Navigate to URL
browser-use back                          # Go back in history
browser-use scroll down                   # Scroll down (--amount N for pixels)
browser-use scroll up                     # Scroll up
browser-use tab list                      # List all tabs
browser-use tab new [url]                 # Open a new tab (blank or with URL)
browser-use tab switch <index>            # Switch to tab by index
browser-use tab close <index> [index...]  # Close one or more tabs

# Page State — always run state first to get element indices
browser-use state                         # URL, title, clickable elements with indices
browser-use screenshot [path.png]         # Screenshot (base64 if no path, --full for full page)

# Interactions — use indices from state
browser-use click <index>                 # Click element by index
browser-use click <x> <y>                 # Click at pixel coordinates
browser-use type "text"                   # Type into focused element
browser-use input <index> "text"          # Click element, clear existing text, then type
browser-use input <index> ""              # Clear a field without typing new text
browser-use keys "Enter"                  # Send keyboard keys (also "Control+a", etc.)
browser-use select <index> "option"       # Select dropdown option
browser-use upload <index> <path>         # Upload file to file input
browser-use hover <index>                 # Hover over element
browser-use dblclick <index>              # Double-click element
browser-use rightclick <index>            # Right-click element

# Data Extraction
browser-use eval "js code"                # Execute JavaScript, return result
browser-use get title                     # Page title
browser-use get html [--selector "h1"]    # Page HTML (or scoped to selector)
browser-use get text <index>              # Element text content
browser-use get value <index>             # Input/textarea value
browser-use get attributes <index>        # Element attributes
browser-use get bbox <index>              # Bounding box (x, y, width, height)

# Wait
browser-use wait selector "css"           # Wait for element (--state visible|hidden|attached|detached, --timeout ms)
browser-use wait text "text"              # Wait for text to appear

# Cookies
browser-use cookies get [--url <url>]     # Get cookies (optionally filtered)
browser-use cookies set <name> <value>    # Set cookie (--domain, --secure, --http-only, --same-site, --expires)
browser-use cookies clear [--url <url>]   # Clear cookies
browser-use cookies export <file>         # Export to JSON
browser-use cookies import <file>         # Import from JSON

# Session
browser-use close                         # Close browser and stop daemon
browser-use sessions                      # List active sessions
browser-use close --all                   # Close all sessions
```

## Multiple Browsers

For subagent workflows or running multiple browsers in parallel, use `--session NAME`. Each session gets its own browser.

## Configuration

```bash
browser-use config list                            # Show all config values
browser-use config set cloud_connect_proxy jp      # Set a value
browser-use config get cloud_connect_proxy         # Get a value
browser-use config unset cloud_connect_timeout     # Remove a value
browser-use doctor                                 # Shows config + diagnostics
browser-use setup                                  # Interactive post-install setup
```

Config stored in `~/.browser-use/config.json`.

## Global Options

| Option | Description |
|--------|-------------|
| `--headed` | Show browser window |
| `--profile [NAME]` | Use real Chrome (bare `--profile` uses "Default") |
| `--cdp-url <url>` | Connect via CDP URL (`http://` or `ws://`) |
| `--session NAME` | Target a named session (default: "default") |
| `--json` | Output as JSON |
| `--mcp` | Run as MCP server via stdin/stdout |

## Tips

1. **Always run `state` first** to see available elements and their indices
2. **Use `--headed` for debugging** to see what the browser is doing
3. **Sessions persist** — browser stays open between commands
4. **CLI aliases**: `bu`, `browser`, and `browseruse` all work
5. **If commands fail**, run `browser-use close` first, then retry

## Troubleshooting

- **Browser won't start?** `browser-use close` then `browser-use --headed open <url>`
- **Element not found?** `browser-use scroll down` then `browser-use state`
- **Run diagnostics:** `browser-use doctor`

## Cleanup

```bash
browser-use close                         # Close browser session
browser-use tunnel stop --all             # Stop tunnels (if any)
```

The owner reviewed devices.html and made two decisions.

**1. Two computers online: option (a), choose in the chat.** Keep only (a):
- Remove option (b) and the review rail's selector for it.
- The page says "You can pair several computers. When more than one is online, choose which one runs the agent in the chat."
- In the chat inset, "Run on: Studio desktop ▾" opens a small popover with the online computers and their presence.
- Add one state: the chosen computer goes offline during a session. The chat shows the round-2 "stopped" notice with New session, and it never switches computers by itself.

**2. Split "Agent connections" into two pages.** The MCP side needs more explanation, and one page holding both is too much:

- **Page 1, "Chat in the editor"** (keep it in devices.html): the Devices section exactly as it is now. Below the intro, one quiet line links to page 2: "Prefer to use Forma from your AI app? Set up MCP →". Remove the MCP, connected applications and access token sections from this page.
- **Page 2, "Use Forma from your AI app"** (new file, `mcp.html`, with the same frame, header and review rail style). Write it for someone who has never heard of MCP:
  - A short intro: what MCP is in one plain sentence, and what you get. Your AI app can create and edit designs in your Forma workspace, and you see them here.
  - "Your MCP URL" with Copy.
  - **Pick your app.** One card per host, each with numbered steps and what to expect:
    - **Claude:** add a custom connector, paste the URL, and approve in the browser. Do not paste a token; it needs none.
    - **ChatGPT and Codex:** connect through the app's own MCP flow with the same URL. These accept OAuth or nothing, so a token will not work.
    - **Command-line tools (Codex CLI, Claude Code, Gemini CLI):** they cannot open a browser, so they need a personal access token. Link to the token section and show an example config line.
  - **Connected applications:** apps you have authorized, with "connected 4 Oct" and Disconnect, using an inline confirmation. Show an empty state and a state with two apps.
  - **Personal access tokens:**
    - say when you need one (command-line tools only);
    - name a key and create it;
    - "Copy this token now. Forma keeps only its hash; you won't see it again.", with Copy;
    - a list showing prefix, created date and Revoke, with an inline confirmation.
  - The review rail switches between: first visit, an app connected, a token just created, and token revoke.
- **Navigation between the two:** both pages sit under one small left navigation (or a two-item switcher under the page title): "Chat in the editor" and "AI apps (MCP)", with the current one marked. No tabs that are really anchors, and no modals.

Same constraints as before: Geist, the Forma "f." mark, Signal only for the primary action, focus, selection and urgent state, plain English, and a specific accessible name on every control. Don't change index.html or chat.html. Re-test both pages after the change.

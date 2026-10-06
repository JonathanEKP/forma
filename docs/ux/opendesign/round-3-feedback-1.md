I tested every scenario of devices.html. These work well:
- the three steps;
- the ticking expiry;
- the escaping of a hostile name in the command;
- the provider summary;
- the "Revoked (12)" group;
- specific Revoke names;
- the inline revoke confirmation;
- both options in E.

Fix these, and change nothing else:

1. **Font.** The page falls back to Inter. Use Geist and Geist Mono, declared as in index.html and chat.html (variable, `font-weight: 100 900`).
2. **The logo.** The header uses a typed "forma" wordmark. Use the same Forma mark as index.html and chat.html, and do not recreate it in a font.
3. **The command loses its spaces after pairing.** In "Paired, not running" it reads `pontis pair--origin https://forma.example--application forma--name …`. Render the command identically in every state, as the exact string that Copy puts on the clipboard.
4. **No tabs.** "Devices / MCP URL / Applications / Access tokens" are anchor links styled as tabs, and the text says "Use the MCP URL below". Keep it one calm page with plain section headings in this order: Devices, Use Forma from an AI app, Connected applications, Personal access tokens. Remove the tab row.
5. **Once a computer is paired, the steps fold away.** In C and E the full three-step panel is still open above the list. With one or more computers paired:
   - the list comes first;
   - "Pair a computer" opens the steps inline, with a name field that starts empty;
   - after pairing, the steps fold back into the new row.
6. **Policy text belongs to its option.** C shows "You can pair several computers. Each one has its own name and connection status." That sentence is option (a)'s policy, and the owner hasn't chosen yet. In C, show no policy sentence. In E, each option shows only its own.
7. **The chat inset in E** has to look like the real chat in chat.html: its composer, its context chip, and, for (b), its "needs you" notice component. Today it is a generic form with a "What would you like to make?" placeholder. For (a), put "Run on: Studio desktop ▾" as a small selector in the composer's footer, where "Ready" sits in chat.html. Opening it lists the online computers with their presence.
8. **Revoke confirmation focus.** Focus goes to the destructive "Revoke" button. Put it on "Keep", and make Escape mean Keep.
9. Re-test every scenario and state after the changes.

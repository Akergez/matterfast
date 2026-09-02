# Parity with the web and mobile clients

What this client does, what it does not, and what each gap costs. Route paths
are `/api/v4` unless noted. Kept by hand — if you close something, move it up.

## Done

**Session** — password, MFA, LDAP, GitLab SSO through the browser, token in the
Secret Service with an announced plaintext fallback, several servers stored and
chosen at launch, sign-out that revokes server-side.

**Reading** — channel feed with author grouping, day separators, edits,
attachments, priority, system messages, webhook name overrides; scrollback a
page at a time, anchored so the view does not jump; a channel with unread
messages opens at them and draws a "New messages" line; threads with CRT
respected; the inbox of mentions, followed threads and saved posts; pinned
messages; search across a team, and `file:` to search attachments.

**Rendering** — Markdown: bold, italic, strike, links, lists, block quotes,
inline code, fenced code on its own panel, tables laid out as rows. Emoji
substituted, mentions tinted, custom emoji drawn inline. Integration and
webhook cards with their fields and colour. Permalink and link previews.

**Writing** — optimistic send, editing, deleting, pinning, saving, marking
read/unread, reminders, forwarding, message priority, acknowledgements,
scheduling and rescheduling, slash commands with their ephemeral replies,
`@` and `:` autocomplete including groups and the server's own emoji, drafts
for channels and threads synced through the server.

**Files** — upload on pick, drag and drop, paste an image, resumable chunked
upload above 8MB, images inline with a full-size viewer, everything else with a
save button.

**Channels and teams** — create, browse, join, leave, rename, set the topic,
archive; members with add and remove; per-channel notification settings; mute;
categories created, renamed, deleted and moved between; bookmarks; browse and
join teams; leave a team; unread badges on the teams you are not looking at.

**Account** — profile and avatar, custom status, presence, account-wide
notification settings, Ctrl+K quick switcher, seven keyboard shortcuts,
background running with a visible toggle and Quit.

**Live** — every websocket event a client acts on: posts, edits, deletes,
reactions, typing both ways, read state, drafts, channel and membership
changes, team membership, threads, acknowledgements, dialogs, plugin notices.
Reliable-reconnect resume, a gap-fill resync, forward pagination to close what
the capped `?since=` leaves behind, and profile refresh for people who changed
while away.

**Storage** — SQLite per server: channels, members, users and messages, written
as they arrive. A cold start draws the last conversation before the network
answers. Deleted on sign-out.

**Notifications** — the server's own rules, including muted channels; keyed per
channel so one conversation cannot bury the desktop; click opens the channel.

**Calls** — join, leave, mute, screen share, camera, raise a hand, react, see
who is in it and what they are doing, live captions, server-side recording,
host controls (mute one or everyone, stop a share, lower a hand, hand over the
host role, remove someone, end the call) and obedience to them when they are
aimed at you. Incoming calls ring with Join and Dismiss, declining a DM call
tells the caller, and answering elsewhere silences this one.

**Plugins** — Agents: bot list, streamed answers, thread summaries, "catch me
up". Reactions-notify: toasts and unread count. Interactive dialogs from any
plugin or slash command.

## Not done

### Worth doing

Nothing outstanding that a desktop client should reasonably do. What remains is
in the two lists below: things that belong to other products or to the server's
own web console, and known ceilings where a cheaper implementation was chosen
on purpose and the reason is written down.

### Deliberately not done

- **Boards and Playbooks.** Separate products with their own plugins; a chat
  client is not where they belong.
- **System console.** Server administration is a web application, not a client
  feature, and the routes are gated on roles this client never has.
- **Integration management** — creating webhooks, slash commands, OAuth apps.
  Same reason.
- **Mobile push.** A desktop client has desktop notifications; push is a
  platform service for phones.
- **Enterprise directory** — group synchronisation, compliance export, shared
  channel administration. Licence-gated server features with no client half.

### Known ceilings

- **Custom emoji in message bodies use a TextView.** Only blocks containing one
  pay for it; the common path stays a Label. Links in that path are
  re-implemented rather than free, and there is no hover cursor over them.
- **Pruning is per channel, not per byte.** The store keeps the newest thousand
  messages in each channel; a server with thousands of channels still grows,
  just slowly and predictably.
- **A second window costs a second session.** `app.new-window` opens one, but
  the session lives inside the window, so two windows means two websockets.
  Correct, not thrifty; sharing one would mean lifting the session out of the
  window and rewriting every handler's ownership.

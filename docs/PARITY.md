# Parity with the web and mobile clients

What this client does, what it does not, and what each gap actually costs.
Route paths are `/api/v4` unless noted. Kept honest by hand — if you close
something here, move it up.

## Done

**Session** — password, MFA, LDAP plugin, GitLab SSO through the browser
(desktop-token flow, `mattermost-dev://` callback), stored token, silent
resume.

**Startup** — the official sequence: config → license → preferences → me →
teams → channels → categories, parallel within each step.

**Reading** — channel feed with author grouping, day separators, edits,
attachments, priority, system messages, webhook overrides; threads with CRT
respected; the inbox of mentions and followed threads; profile cards; custom
status; presence.

**Writing** — optimistic send with the echo retiring the local copy, replies
into threads, reactions, editing, deleting, pinning, saving, marking unread,
file attachments uploaded on pick.

**Drafts** — server-side, synced, with the `Connection-Id` echo guard and the
501 fallback to local-only when an admin has turned them off.

**Search** — post search per team, results in the right panel.

**Live** — posts, edits, deletes, reactions, presence, typing (both
directions), read state from other sessions, draft changes, channel and
membership changes, team membership. Reliable-reconnect resume, with a
gap-fill resync when the server cannot replay.

**Notifications** — desktop toasts with the server's own rules (channel
setting over account setting, DM counts as a mention, nothing for the channel
you are reading while focused), clicking one opens the channel.

**Calls** — join, leave, mute, screen share, camera, raise a hand, server-side
recording, remote video, a participant list with who is talking, muted, queued
to speak or sharing, and a dock that survives leaving the channel.

**Plugins** — Agents (`mattermost-ai`): bot list, streamed answers, summarise
thread, catch me up. Reactions-notify (`ru.toxblh.reactions-notify`): toasts
and unread count.

## Not done

Ordered by how often it bites.

### Sending and reading

- **Message priority and acknowledgements.** Rendered, not settable. Needs the
  priority controls in the composer and `POST/DELETE /users/{u}/posts/{p}/ack`.
- **Scheduled posts.** `POST /posts/schedule`, `PUT|DELETE /posts/schedule/{id}`,
  `GET /posts/scheduled/team/{team}`.
- **Post reminders.** `POST /users/{u}/posts/{p}/reminder`.
- **Thread drafts.** The draft store is keyed by `(channel, root)` server-side
  and this client only reads and writes the `root == ""` half, because the
  thread panel's composer is separate.
- **Edit history.** `GET /posts/{p}/edit_history`, `POST /posts/{p}/restore/{v}`.
- **Move a thread.** `POST /posts/{p}/move`.
- **Permalink previews.** A link to a post renders as a link, not as the post.
- **Markdown.** Messages render as plain text with emoji substituted. No bold,
  no code blocks, no tables, no links-as-links.
- **@-mention and emoji autocomplete** in the composer.

### Files

- **Chunked uploads.** Single-shot multipart only, so a large file that drops
  mid-upload starts over. `POST /uploads` + `GET|POST /uploads/{id}` with
  `file_offset` is the resumable path.
- **Video and audio previews.** Images show inline and open full size;
  everything else is a filename and a size.
- **Drag and drop**, and paste-to-upload.

### Channels and teams

- **Create, join, leave, rename, archive** — none of it. `POST /channels`,
  `POST /channels/{id}/members`, `DELETE /channels/{id}/members/{u}`,
  `PUT /channels/{id}`, `DELETE /channels/{id}`.
- **Channel members list** and adding people.
- **Channel notification settings.**
  `PUT /channels/{c}/members/{u}/notify_props`.
- **Sidebar category editing** — categories are read and honoured, never
  changed. `POST|PUT|DELETE /users/{u}/teams/{t}/channels/categories`.
- **Channel bookmarks** (server 9.4+). `/channels/{c}/bookmarks`.
- **Browsing and joining teams.**

### Search and discovery

- **Search filters** (`in:`, `from:`, date ranges) work because the server
  parses them out of the terms, but nothing in the UI suggests them.
- **File search.** `POST /teams/{t}/files/search`.
- **Channel and user search**, and the switcher (Ctrl+K) they feed.
- **Saved posts list.** Saving works; there is no list to read them back from.
  `GET /users/{u}/posts/flagged`.
- **Pinned posts list.** Pinning works; the list does not.

### Account

- **Notification settings.** Read and obeyed, never edited.
  `PUT /users/{u}/patch` with `notify_props`.
- **Set your own status** (online/away/dnd). Presence is shown, not set.
  `PUT /users/{u}/status`.
- **Profile editing**, avatar upload.
- **Multi-server.** One account at a time; the switcher popover has room for
  more but nothing behind it.
- **Session in the keyring.** The token is a 0600 file, not libsecret.

### Calls

Sixteen of the twenty-one `CallsEvent` variants are parsed and ignored. The
ones that matter:

- **Host controls** — `HostChanged`, `HostMuteRequest`, `HostRemoved` and the
  rest.
- **Ringing and dismissal** — `POST /calls/{c}/dismiss-notification`, and no
  incoming-call UI.
- **Transcription and live captions** — the job routes and `JobState`.

### Platform

- **A real local store.** There is a snapshot — last session's channels and a
  screenful of posts each, drawn before the network answers — but no database,
  so history beyond that is always a fetch and nothing is searchable offline.
- **Background/tray operation.**
- **Push notifications** on mobile builds.
- **Group mentions** (`@group`), licensed servers only.
- **Custom emoji** are fetched but not offered in the picker.

## Performance notes

The target is that no interaction waits on the network before showing
something. Where that holds today:

- Sending is optimistic; the echo replaces the local copy.
- Reactions, saving and marking read all apply locally first and roll back if
  the write fails.
- A channel already read is switched to instantly — the feed is cached in
  memory for the session.
- Drafts save on a 900ms debounce, not per keystroke; typing notifications go
  out at most once every three seconds.
- Sidebar reloads are debounced at 400ms so a burst of membership events costs
  one round of requests.

Where it does not:

- **First open of a channel** waits for `posts?per_page=60` unless the snapshot
  happened to keep it. It shows a spinner, which is honest, but a real store
  would show the last known messages for every channel rather than eight.
- **Cold start** draws the cached snapshot immediately; the first *ever* launch
  on a machine still waits for the startup sequence.
- **Search** has no local index, so every search is a round trip.

# Parity with the web and mobile clients

What this client does, what it does not, and what each gap actually costs.
Route paths are `/api/v4` unless noted. Kept honest by hand — if you close
something here, move it up.

## Done

**Session** — password, MFA, LDAP plugin, GitLab SSO through the browser
(desktop-token flow, `mattermost-dev://` callback), silent resume. The token
lives in the Secret Service, with an announced plaintext fallback for machines
with no keyring running. Several servers can be signed in at once; a launch
with more than one stored asks which.

**Startup** — the official sequence: config → license → preferences → me →
teams → channels → categories, parallel within each step.

**Reading** — channel feed with author grouping, day separators, edits,
attachments, priority, system messages, webhook overrides; Markdown (bold,
italic, strikethrough, links, headings, nested lists, quotes, inline code and
fenced blocks); permalinks rendered as the message they point at; edit history;
threads with CRT respected; the inbox of mentions, followed threads and saved
posts; pinned posts; profile cards; custom status; presence.

**Writing** — optimistic send with the echo retiring the local copy, replies
into threads, reactions from an emoji picker — with the server's own uploads
drawn as pictures in the chips rather than printed as `:shipit:` — editing, deleting, pinning,
saving, marking unread, moving a thread to another channel, message priority
set from the composer, acknowledging a post that asks for one, scheduled
messages with a list to manage them, post reminders, `@mention` and `:emoji`
autocomplete.

**Files** — attach from the picker, drag and drop, or paste an image.
Anything over the chunk size goes through a resumable upload session, so a
large file that stalls resumes from where the server got to rather than from
zero. Images draw themselves inline and open full size.

**Drafts** — server-side and synced, for the channel composer *and* the thread
composer, with the `Connection-Id` echo guard and the 501 fallback to
local-only when an admin has turned them off.

**Channels** — create, browse, join, leave, rename, edit the header, archive.
Member list with search, add and remove. Per-channel notification settings.
Mute or re-file a channel from its own row in the sidebar. Bookmarks. Browse
and join teams.

**Account** — edit your profile and upload an avatar, set your own status and
a custom one, edit the account notification settings that drive every toast.

**Search** — post search per team with the results in the right panel, and
`file:` in front of the terms to search attachments instead. Ctrl+K jumps to a
channel or a person, opening the DM if there is not one yet.

**Live** — posts, edits, deletes, reactions, presence, typing (both
directions), read state from other sessions, draft changes, channel and
membership changes, team membership. Reliable-reconnect resume, with a
gap-fill resync when the server cannot replay.

**Notifications** — desktop toasts with the server's own rules (channel
setting over account setting, DM counts as a mention, nothing for the channel
you are reading while focused), clicking one opens the channel. Closing the
window keeps the process, the socket and the toasts alive; that is one setting
and it defaults to on.

**Calls** — join, leave, mute, screen share, camera, raise a hand, server-side
recording, remote video, a participant list with who is talking, muted, queued
to speak or sharing, and a dock that survives leaving the channel. A call
starting in a DM or group rings, with Join and Dismiss on the notification.
Host controls: mute someone, remove them.

**Plugins** — Agents (`mattermost-ai`): bot list, streamed answers, summarise
thread, catch me up. Reactions-notify (`ru.toxblh.reactions-notify`): toasts
and unread count.

## Not done

Ordered by how often it bites.

### Reading history

- **Scrollback.** The biggest one. A channel is opened with
  `posts?per_page=60` and that is every message you will ever see in it; the
  scroller hits the top and stops. Anything older than about a day in a busy
  channel is unreachable from this client. `GET /channels/{c}/posts` with
  `before` is the route, and it needs `ChannelFeed` to become a list of blocks
  merged on overlap rather than one contiguous run — the note in `state.rs`
  says why.
- **Jump to a message, and to the first unread.** The permalink card opens the
  thread it belongs to, not the point in the channel where it was said, and
  there is no "you left off here" line. `GET /users/me/channels/{c}/posts/unread`
  is the route; it needs the same block merging.
- **Link previews.** Only `permalink` embeds are drawn. A shared link is a
  link, so an image or an article posted in a channel says nothing about
  itself until you leave the app to look.
- **Tables.** Parsed, then flattened — the cells run together into one line,
  which is worse than showing the pipes. Needs a `Block::Table` and a
  `gtk::Grid`.

### Messages

- **`@name` in a message body is plain text.** Not tinted, not clickable. You
  cannot see at a glance which message in a run mentions you, and reaching a
  person's card means finding a post they wrote.
- **Non-image attachments cannot be opened or saved.** A PDF is a filename and
  a size and nothing else — no download button, no "open with". `download_file`
  is already there and used for images; this is a menu and a
  `gtk::FileDialog::save`. Video and audio have no player either.
- **Slash commands.** `/away`, `/giphy`, anything a plugin registers — typing
  one posts it as literal text, in public, which is the kind of mistake a chat
  client should not make possible. `POST /commands/execute` is not in the API
  crate yet, and the ephemeral reply that answers it is an event this client
  ignores.
- **Requesting an acknowledgement.** You can acknowledge a post that asks for
  one; you cannot ask. The composer sets `priority` and sends
  `requested_ack: None`.
- **Restoring an old version of a post.** The history is readable, the
  versions in it are not restorable. `POST /posts/{p}/restore/{v}`.
- **Rescheduling a scheduled message.** It can be created, listed and deleted,
  not moved. `PUT /posts/schedule/{id}`.
- **Shortcodes in a message body are never substituted.** The emoji table is
  used for reaction chips, quick reactions and custom statuses, and nowhere
  else — so `:tada:` typed into a message stays `:tada:` on screen, which is
  most of the emoji anyone actually sends. `emoji::resolve` and
  `Avatars::custom_emoji` are already there; the message renderer does not call
  them.
- **Group mentions do not complete.** `@`-completion asks
  `/users/autocomplete` for people only. `mentionable_groups` is in the API
  crate with nothing calling it, so on a licensed server a group has to be
  typed exactly right from memory. `GET /groups`.
- **The picker and the `:emoji` autocomplete are Unicode-only.** Both iterate
  the `emojis` crate, so a server's own uploads can be reacted with only by
  finding an existing chip. `GET /emoji/search` and `GET /emoji/autocomplete`.
- **Clearing a draft** upserts an empty one rather than
  `DELETE /drafts/channel/{c}`.

### Threads and unread state

- **Following and unfollowing a thread.** `POST|DELETE /users/{u}/teams/{t}/threads/{id}/following`.
  The inbox lists the threads the server says you follow; there is no way to
  add one you care about or drop one you do not.
- **Marking a thread read.** `PUT /users/{u}/teams/{t}/threads/{id}/read/{time}`.
  Reading a thread does not clear its unread count, so the inbox badge only
  ever grows.
- **Unread on the other teams.** The team rows in the switcher carry no dot
  and no count, so a mention in another team is invisible until you switch to
  it. `GET /users/me/teams/unread`.
- **Live thread and acknowledgement updates.** `thread_updated`,
  `thread_follow_changed`, `post_acknowledgement_added` and `_removed` all
  arrive as `Event::Other` and are dropped, so those parts of the screen are
  correct only until something else refreshes them.
- **Ephemeral messages.** Dropped, which is why a plugin that answers you
  privately appears to answer nothing.

### Channels and discovery

- **Categories can be re-filled, not managed.** A channel can be moved into an
  existing category from its row; creating, renaming, deleting and reordering
  categories are not there.
  `POST|PUT|DELETE /users/{u}/teams/{t}/channels/categories`.
- **Leaving a team.** Joining works. `DELETE /users/{u}/teams/{t}/members/{u}`.
- **Member count** on a channel. `GET /channels/{c}/stats`.
- **The search box suggests nothing.** `in:`, `from:`, date ranges and this
  client's own `file:` prefix all work, because the server parses the first
  three out of the terms and the client strips the last — but nothing in the
  UI says so, so in practice nobody uses them.

### Calls

- **Host requests aimed at you are ignored.** `HostMuteRequest`,
  `HostScreenOffRequest`, `HostLowerHandRequest` and `HostRemoved` are parsed
  and dropped. Host controls are advisory — the server asks the client to
  comply and this one does not — so a host muting you does nothing at all.
- **Dismissal does not propagate.** `UserDismissedNotification` is dropped and
  the ring notification is never withdrawn, so answering a call on your phone
  leaves this one still ringing on the desktop.
- **Transcription and live captions.** `JobState` is handled for recordings
  and returns early for everything else.

### Platform

- **A real local store.** There is a snapshot — last session's channels and a
  screenful of posts each, drawn before the network answers — but no database,
  so history beyond that is always a fetch and nothing is searchable offline.
  This is also what would make scrollback cheap rather than chatty.
- **One account at a time.** Several servers can be stored and one is chosen
  at launch; there is no switching between them without signing out, and no
  merged view.
- **Running in the background is invisible and cannot be turned off.** Closing
  the window keeps the process alive, which is right, but nothing says so:
  there is no tray icon, no menu entry for it, and the `background` setting is
  a boolean in `settings.json` with no UI. Quitting is Ctrl+Q — `app.quit`
  exists and no menu names it — so somebody who closes the window and expects
  it gone has no way to find out otherwise. The toggle belongs in the
  notification settings dialog and the Quit entry in the main menu.
- **Push notifications** on mobile builds.

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
- Autocomplete answers from what is already loaded before the server's list
  arrives, so a keystroke never blocks on a request.

Where it does not:

- **First open of a channel** waits for `posts?per_page=60` unless the snapshot
  happened to keep it. It shows a spinner, which is honest, but a real store
  would show the last known messages for every channel rather than eight.
- **Cold start** draws the cached snapshot immediately; the first *ever* launch
  on a machine still waits for the startup sequence.
- **Search** has no local index, so every search is a round trip.
- **Uploads are read into memory whole** before the first chunk goes out, so a
  very large file costs its own size in RAM.

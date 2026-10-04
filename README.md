# Matterfast

A native Mattermost client for the Linux desktop, in Rust. The interface is
drawn with [GPUI](https://crates.io/crates/gpui-kit) and its component kit.

Matterfast is a fork of [Matras](https://github.com/Toxblh/matras) by Anton
Palgunov.

<img src="/docs/screenshots/main.png" />

Four crates:

| Crate | What it is |
|---|---|
| `mattermost-api` | Async Mattermost client: REST v4, models, and a reliable WebSocket with replay-aware reconnect. No UI dependency. |
| `mattermost-calls` | The Mattermost Calls wire protocol and a WebRTC peer (webrtc-rs). No UI dependency. |
| `matterfast` | The application: one adaptive window, from desktop width down to a phone. |
| `matterfast-testserver` | A fake Mattermost, real enough to exercise the client end to end. Dev-only. |

The two library crates are deliberately independent of the UI — they are usable
for a bot, a CLI, or a different front end.

## Status

**Working**

- Sign in (password + MFA), session token handling, `X-Requested-With`, no cookie jar
- **Single sign-on** through the browser, using Mattermost's desktop-token
  flow — see below
- The session token lives in the **Secret Service**, not a file, because it
  grants the whole account until somebody revokes it. Several servers can be
  stored side by side; a launch with more than one asks which
- Startup sequence in the order the official clients use it (config → teams → channels → categories)
- Adaptive layout: channel sidebar, conversation, and a right-hand panel that
  holds either a thread or the inbox; teams and the signed-in account live in a
  switcher popover in the sidebar header
- A thread earns a static third column when there is room; the inbox always
  overlays, because a stack you glance at should not shove the conversation
  aside
- **Call dock** pinned under the channel list: who is talking, how many are in
  it, the way back to its channel, and every in-call control. A call outlives
  the channel it started in, so its controls cannot live in that channel's
  header. On a phone-sized window the dock moves under the conversation
- Sidebar rows carry the faces of whoever is in a channel's call
- Sidebar categories with the real ordering rules, unread/mention/muted
  styling, and a row menu to mute a channel or move it to another category
- Virtualized message list: only the visible screenful is laid out, while
  author grouping, day separators, edits, attachments, message priority,
  system messages and webhook name overrides remain intact
- **Profile pictures and media**, shared in memory and persisted in an
  expiring disk cache. Storage has a 5 GiB default global limit, LRU eviction,
  and a Storage panel where the limit can be changed or the cache cleared
- **Reactions as emoji**, not `:shortcodes:` — click a chip to toggle your own,
  pick one from the hover menu, or search the whole Unicode set. A server's own
  uploads are drawn as pictures, through the same image cache as avatars
- **Threads**: the reply count opens the thread in the right panel, replies go
  to the root's channel, and collapsed reply threads (CRT) are respected — a
  reply never leaks into the channel feed. The thread's reply box keeps its own
  synced draft, the way the server stores them
- **Profile card** on any avatar or name: picture, presence, position, custom
  status, the person's local time, and a button that opens the DM
- **Inbox**: recent mentions, followed threads and saved posts, with a count on
  the header button
- Sending messages, optimistically, with the server echo retiring the local copy
- **Editing, deleting, pinning, saving and marking unread**, from each
  message's own menu — plus edit history, reminders, and moving a whole thread
  to another channel
- **Message priority** set from the composer, and a button to acknowledge a
  post that asked for one
- **Scheduled messages**, with a list to change your mind before they go
- **Autocomplete** for `@names` and `:emoji`, answering from what is already
  loaded first so a keystroke never waits on a request
- **Markdown**: bold, italic, strikethrough, links, headings, nested lists,
  quotes, inline code, and fenced code blocks on their own panel
- **Permalinks** render as the message they point at, since the server has
  already resolved it and fetching it again would be work for nothing
- **Attachments**: pick, drag in, or paste an image. Anything large goes
  through a resumable upload session, so a stalled upload continues from where
  the server got to instead of starting over. Images draw themselves and open
  full size
- **Drafts**, synced through the server so the same unfinished message is
  waiting on every device
- **Channels**: create, browse, join, leave, rename, edit the header, archive;
  a member list you can add to and remove from; per-channel notification
  settings; bookmarks. Teams can be browsed and joined
- **Your account**: edit the profile and the avatar, set a status and a custom
  status, and edit the notification settings that decide every toast
- **Search** across a team, with results in the right panel — `file:` in front
  of the terms searches attachments instead, rather than adding a second box to
  find
- **Themes**: light, dark or whatever the desktop is, with a theme of your
  choice for each. Besides the built-in pair, themes are the Zed editor's:
  none is shipped, the settings browse and install them from Zed's extension
  registry, and a Zed theme file dropped into
  `~/.local/share/io.gitlab.akergez.Matterfast/themes/` is picked up at the
  next start
- **Ctrl+K** jumps to a channel or a person, opening the DM if there is not one
  yet
- **Desktop notifications** following the server's own rules, and a snapshot
  cache so launching draws last session's channels immediately. Closing the
  window keeps the process, the socket and the notifications alive — a chat
  client you have to keep a window open for is not one
- **Calls**: join, mute, share a screen or camera, raise a hand, see who is in
  the call, and a dock that stays put when you read somewhere else. A call in a
  DM or a group rings, with Join and Dismiss on the notification; a host can
  mute or remove someone
- **Plugins**: the Agents LLM module (streamed answers, thread summaries,
  "catch me up") and reaction notifications
- Live updates over the websocket, including a correct reliable-reconnect resync

**Not done yet**

The honest list lives in [docs/PARITY.md](docs/PARITY.md) — what is missing,
ordered by how often it bites, with the routes that would close each gap. The
short version: no scrollback past the newest sixty messages in a channel, no
link previews, no slash commands, non-image attachments cannot be saved,
`:shortcodes:` typed into a message stay as text, and there is a snapshot cache
rather than a real local store.

## Single sign-on

The "Single sign-on" button asks the server which providers it has enabled —
OpenID Connect, SAML, GitLab, Google, Entra ID — and sends the browser to that
one's login route with `?desktop_token=…` (`/oauth/<service>/login`, or
`/login/sso/saml`). With several enabled, it offers a button for each.
The server cannot redirect an OAuth callback to a loopback port — it insists the
redirect matches the site's own scheme and host — so the answer comes back
through a URL scheme instead: Mattermost bounces the browser to a page that
deep-links to `mattermost-dev://…`, which launches this app, and the token in
that URI is traded for a session.

That only works once the desktop file is installed, so the browser knows what
handles the scheme. A packaged build gets this for free. From a source tree:

```sh
cargo build
sed "s|^Exec=matterfast|Exec=$PWD/target/debug/matterfast|" \
  data/io.gitlab.akergez.Matterfast.desktop \
  > ~/.local/share/applications/io.gitlab.akergez.Matterfast.desktop
update-desktop-database ~/.local/share/applications
xdg-mime default io.gitlab.akergez.Matterfast.desktop x-scheme-handler/mattermost-dev
```

On Windows there is no installer and nothing to install: the application
registers the scheme for the signed-in user every time it starts
(`HKEY_CURRENT_USER\Software\Classes\mattermost-dev`, `url_scheme.rs`), so
the entry follows the executable when the folder is moved. The second launch
the browser causes reaches the running copy through a named pipe
(`ipc_windows.rs`), the counterpart of the socket used on Linux.

The scheme is `mattermost-dev`, not `mattermost`, so nothing collides with the
official desktop app; the server picks it because the token starts with `dev-`.
The app must already be running when the callback arrives — the browser hands
the URI to the live instance over D-Bus.

## Install

A signed flatpak repository is published from this project's own CI. One click
on the ref file adds the remote and installs the app; everything after that is
an ordinary `flatpak update`:

```sh
flatpak install https://matterfast-63d8e7.gitlab.io/io.gitlab.akergez.Matterfast.flatpakref
```

The runtime comes from Flathub, so that remote has to exist — the ref file
points at it and flatpak will offer to add it.

Without flatpak, each [release](https://gitlab.com/ragusseven/matterfast/-/releases)
carries a tarball for x86_64 and for aarch64, a Windows zip for x86_64, an
Android package and a macOS disk image for Apple silicon. The tarball is laid out like an install
prefix, so unpacking it is the installation; the libraries listed under
"Building" have to come from the distribution:

```sh
tar -xzf matterfast-1.0.1-linux-x86_64.tar.gz --strip-components=1 -C ~/.local
```

The macOS application is not signed by an Apple developer account, so a
downloaded copy is held back until its quarantine mark is taken off:

```sh
xattr -dr com.apple.quarantine /Applications/Matterfast.app
```

## Building


Needs a recent stable Rust (1.95 is what it is developed with), a Vulkan
driver to run, and the development files for Wayland/X11 input, fonts, ALSA,
GStreamer and Opus to build.

```sh
# Debian/Ubuntu
sudo apt install build-essential pkg-config libssl-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libvulkan-dev \
  libfontconfig-dev libfreetype-dev libasound2-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libopus-dev

# Fedora
sudo dnf install gcc pkgconf-pkg-config openssl-devel \
  wayland-devel libxkbcommon-devel libxkbcommon-x11-devel libxcb-devel vulkan-loader-devel \
  fontconfig-devel freetype-devel alsa-lib-devel \
  gstreamer1-devel gstreamer1-plugins-base-devel opus-devel

cargo build --release
cargo test --workspace
```

Run it:

```sh
./target/release/matterfast
```

To look at the layout without a server:

```sh
MATTERFAST_DEMO=1 ./target/release/matterfast
```

## UI tests

`crates/matterfast/tests/ui/` holds scenarios: scripts of clicks, keystrokes
and `expect:` checks that are played into the real application, against the
demo data or a fresh `matterfast-testserver`.

```sh
build-aux/ui-tests.sh --headless          # all of them, under a private headless sway
build-aux/ui-tests.sh --headless theme    # one
build-aux/ui-tests.sh                     # on your own display
```

`--headless` needs `sway` and a Vulkan driver (Mesa's `lavapipe` will do) and
is how CI runs them. It matters for scenarios that click: coordinates only
mean something at the window size the scenario names, and a tiling desktop
will not give the window that size. Checks read the session — the open
channel, whether a dialog is up, the theme — not pixels, so they do not
depend on fonts or the GPU. The step list is at the top of
`crates/matterfast/src/ui/script.rs`.

## Testing against a server

`matterfast-testserver` is a fake Mattermost with real state: it hands out real ids,
serves avatars, keeps threads and reactions, and pushes the same websocket
events the real server does — including the double-encoded payloads and the
inconsistent key casing, because reproducing those is the point.

```sh
cargo run -p matterfast-testserver          # 127.0.0.1:8065, logs every request

MATTERFAST_SERVER=http://127.0.0.1:8065 \
MATTERFAST_USER=anton MATTERFAST_PASSWORD=test \
  cargo run -p matterfast               # skips the sign-in form
```

A background "colleague" posts every 12 seconds (`MM_BOT_SECONDS` to change it)
so live updates are visible with one client open. The same three environment
variables work against a real server.

> On a minimal system, install `librsvg2-common` too — without the SVG pixbuf
> loader every symbolic icon renders as a broken-image box, which looks like an
> application bug and is not one.

## Using the libraries

```rust
use mattermost_api::{Client, WebSocket, ws::{WsUpdate, Event}};

let client = Client::new("https://mm.example.com")?;
client.login("alice", "hunter2", None).await?;

let ws = WebSocket::connect(client.websocket_url(), client.token().unwrap());
let mut rx = ws.subscribe();

while let Ok(update) = rx.recv().await {
    match update {
        WsUpdate::Event(Event::Posted(p)) => println!("{}: {}", p.sender_name, p.post.message),
        // The server could not replay its buffer — refetch.
        WsUpdate::MissedMessages => { /* full resync */ }
        _ => {}
    }
}
```

Joining a call:

```rust
use mattermost_calls::{config, CallSession, JoinOptions};

let discovery = config::discover(&client).await?;
let call = CallSession::join(
    &client,
    ws,
    JoinOptions { channel_id: channel_id.into(), ..Default::default() },
    &discovery,
).await?;

let track = call.unmute().await?;   // then write Opus samples into `track`
```

## Notes on the protocol

Mattermost's REST API is documented; its WebSocket payloads and the Calls
protocol largely are not. `docs/PROTOCOL.md` records what was found by reading
`mattermost/mattermost`, `mattermost-plugin-calls`, `rtcd` and `calls-common`,
including several behaviours that fail *silently* if you get them wrong.

## Continuous integration

`.gitlab-ci.yml` runs on GitLab CI, on Linux runners and one Mac.

| When | What |
|---|---|
| every commit, on a branch or in a merge request | `cargo test --workspace`, the UI scenarios under a headless compositor, then the Linux tarballs, the Windows zip, the Android package and the macOS disk image, kept as job artifacts |
| a version tag, `v1.2.3` | the same tests, then every package, the flatpak repository on GitLab Pages, and a release |

A commit's packages are built with the `release` profile and are not
published anywhere; the flatpak is built from a tag only. A tag pipeline
builds, with the `dist` profile:

- the flatpak for x86_64 and aarch64, natively on a runner of each
  architecture;
- a Linux tarball for each of the two, likewise;
- a Windows zip for x86_64, cross-compiled on Linux
  (`build-aux/build-windows.sh`). It has no video yet, and there is no
  aarch64 one: a dependency does not cross-compile for it;
- an Android package for arm64, cross-compiled on Linux
  (`build-aux/build-android.sh`), signed with the release key;
- a macOS disk image for Apple silicon, built on a Mac
  (`build-aux/package-macos.sh`).

Each file goes into the project's package registry under the version, and
the release links to them and carries the flatpak install command. The
Android package and the macOS bundle take their version from the tag.

The jobs pick their runner by tag: `linux-arm64` for the aarch64 Linux builds,
`macos-arm64` for the macOS one, `linux-x86` for everything else, the Android
build included: the NDK exists for no other Linux. The flatpak jobs need a
privileged container, because flatpak-builder sandboxes the build with bwrap.

The Mac is a runner with a shell executor, and has to have what a job cannot
install for itself: Xcode (the toolkit's shaders are compiled by its Metal
compiler, which is also why this build is not cross-compiled), rustup, and
`brew install cmake librsvg`.

### Signing the Android package

Android installs an update only over a package signed with the same key, so
every release is signed with one key, and a tag pipeline fails without it.
A commit's package is signed with a debug key made on the spot: it installs,
but not over a release, and not over another commit's.

```sh
keytool -genkeypair -storetype PKCS12 -keystore matterfast.p12 -alias matterfast \
  -keyalg RSA -keysize 4096 -validity 10000 -dname 'CN=Matterfast'
base64 < matterfast.p12 | tr -d '\n'
```

Keep the keystore: a lost key means nobody can update without uninstalling
first. Two CI/CD variables, masked, and protected only if the `v*` tags are:

| Variable | Value |
|---|---|
| `ANDROID_KEYSTORE_B64` | the base64 above |
| `ANDROID_KEYSTORE_PASSWORD` | the keystore's password |

### Making a release

The tag is the version, and `build-aux/check-version.sh` fails the pipeline
before anything is built if the crates say something else.

```sh
# 1. one version for every crate
sed -i 's/^version = ".*"/version = "1.2.3"/' Cargo.toml   # under [workspace.package]
cargo update --workspace                                   # the same in Cargo.lock
build-aux/check-version.sh v1.2.3                          # prints 1.2.3 when they agree

# 2. commit, tag that commit, push both
git commit -am 'Release 1.2.3'
git tag v1.2.3
git push origin master v1.2.3
```

### Publishing the flatpak

The two flatpak jobs run one after the other and build into the same ostree
repository, which travels between them as a job artifact. The `pages` job then
signs it, packs each architecture as a single `.flatpak` file for the package
registry, and lays the repository out as the Pages site: the repository under `/repo`, and
`io.gitlab.akergez.Matterfast.flatpakref` beside it with the public key
embedded, so the install link above always carries the key that signed what
it points at. There is no other host and no deploy key: Pages is served from
the pipeline's own artifact.

Three CI/CD variables drive it. Mask them. If you also protect them, protect
the `v*` tags too (Settings → Repository → Protected tags), or the tag
pipeline will not be given them:

| Variable | Value |
|---|---|
| `FLATPAK_GPG_ID` | fingerprint of the signing key |
| `FLATPAK_GPG_KEY_B64` | `gpg2 --export-secret-keys <fpr> \| base64 -w0` |
| `FLATPAK_GPG_PASSPHRASE` | that key's passphrase |

The signing key never enters a personal keyring: it lives as an exported
blob beside its passphrase, and every publish imports it into a throwaway
`GNUPGHOME`. That homedir also gets a stub pinentry, because ostree signs
through gpgme, which cannot hand a passphrase to gpg-agent itself, and the
build image has no pinentry to prompt with.

The same script builds locally, into `.flatpak-repo`:

```sh
build-aux/publish-flatpak.sh
flatpak install --user .flatpak-repo io.gitlab.akergez.Matterfast
```

and the tarball is one more command after a dist build:

```sh
cargo build --profile dist --locked -p matterfast
build-aux/package-tarball.sh            # dist/matterfast-<version>-linux-<arch>.tar.gz
```

## Licence

GPL-3.0-only. See `LICENSE`.

Based on [Matras](https://github.com/Toxblh/matras) by Anton Palgunov.

### Bundled fonts

The binary carries two fonts, each under its own licence; the texts are in
`crates/matterfast/assets/fonts/` and are installed beside the application.

- [Inter](https://github.com/rsms/inter) 4.1, © The Inter Project Authors,
  [SIL Open Font License 1.1](https://openfontlicense.org). Unmodified.
- [Twemoji Mozilla](https://github.com/mozilla/twemoji-colr) 0.7.0. The emoji
  graphics are [Twemoji](https://github.com/twitter/twemoji), © Twitter, Inc
  and other contributors,
  [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/); the font build
  is © Mozilla Foundation,
  [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
  Modified: only the names inside the font file were changed, to
  `Noto Color Emoji`, because the text renderer draws colour glyphs only
  from a font with that name. It is not Noto Color Emoji and is not
  affiliated with Google's Noto project.

Matterfast is an independent client. It is not affiliated with, endorsed by, or
sponsored by Mattermost, Inc.; "Mattermost" is used only to name the server
protocol it speaks.

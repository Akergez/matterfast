Research complete. Here's the full report.

---

# Mattermost official clients — architecture research for a GTK4/libadwaita Rust client

Repos cloned to `/tmp/mm/mattermost-mobile`, `/tmp/mm/desktop`, `/tmp/mm/mattermost` (webapp + client package).

---

## 1. mattermost-mobile (React Native + WatermelonDB)

This is the closest analogue to what you're building: a **native, offline-first, locally-persisted** client. Steal its data model.

### 1.1 Local database

WatermelonDB (SQLite) with two databases: an **app database** (`app/database/schema/app/`) holding the server list, credentials pointers, global state; and one **server database per server URL** (`app/database/schema/server/index.ts`, `DatabaseManager.serverDatabases[serverUrl]`).

Column definitions are one file per table in `app/database/schema/server/table_schemas/`; matching models with relations in `app/database/models/server/`. Every table's primary key is the server-side ID (WatermelonDB `id`), which is why several relations are `has_many foreignKey:'id'` — that's a 1:1 keyed on the same ID.

**Entities (table → columns):**

| Table | Columns |
|---|---|
| `Channel` | create_at, creator_id*, delete_at, display_name, is_group_constrained, name*, shared, team_id*, type, update_at, banner_info?, abac_policy_enforced?, autotranslation? |
| `ChannelInfo` (1:1 w/ Channel, id=channel id) | guest_count, header, member_count, pinned_post_count, files_count, purpose |
| `MyChannel` (1:1, id=channel id — *my membership*) | is_unread, last_post_at, last_viewed_at, manually_unread, mentions_count, message_count, roles, viewed_at, last_fetched_at*, last_playbook_runs_fetch_at, autotranslation_disabled? |
| `MyChannelSettings` (1:1) | notify_props (JSON) |
| `ChannelMembership` (other people) | channel_id*, user_id*, scheme_admin |
| `ChannelBookmark` | create/update/delete_at, channel_id*, owner_id, file_id?, display_name, sort_order, link_url?, image_url?, emoji?, type, original_id?, parent_id? |
| `Team` | allowed_domains, description, display_name, is_allow_open_invite, is_group_constrained, last_team_icon_updated_at, name, type, update_at, invite_id |
| `MyTeam` (1:1) | roles |
| `TeamMembership` | team_id*, user_id*, scheme_admin |
| `TeamChannelHistory` (1:1 per team) | channel_ids (JSON array — MRU stack) |
| `TeamSearchHistory` | created_at, display_term, team_id*, term |
| `Category` | collapsed, display_name, muted, sort_order, sorting, team_id*, type |
| `CategoryChannel` (N:N join) | category_id*, channel_id*, sort_order |
| `Post` | channel_id*, create_at, delete_at, edit_at, is_pinned, message, message_source, metadata (JSON)?, original_id, pending_post_id*, **previous_post_id**, props, root_id, type*, update_at, user_id* |
| `PostsInChannel` | channel_id*, earliest, latest — **the gap/chunk table** |
| `PostsInThread` | root_id*, earliest, latest |
| `Reaction` | create_at, emoji_name, post_id*, user_id* |
| `File` | extension, height, image_thumbnail, local_path?, mime_type, name, post_id*, size, width, is_blocked |
| `Draft` | channel_id*, files, message, root_id*, metadata?, update_at, type? |
| `ScheduledPost` | channel_id*, files, message, root_id*, metadata?, create/update_at, scheduled_at, processed_at, error_code, type? |
| `User` | auth_service, delete_at, email, first_name, is_bot, is_guest, last_name, last_picture_update, locale, nickname, notify_props, position, props, remote_id?, roles, status, timezone, update_at, username, terms_of_service_id, terms_of_service_create_at |
| `Thread` (CRT; id = root post id) | is_following, last_reply_at, last_viewed_at, reply_count, unread_mentions, unread_replies, viewed_at, last_fetched_at* |
| `ThreadParticipant` | thread_id*, user_id* |
| `ThreadsInTeam` | team_id*, thread_id* |
| `TeamThreadsSync` (1:1 per team) | earliest, latest |
| `Preference` | category*, name, user_id*, value |
| `Role` | name*, permissions |
| `System` | value (single key/value blob table) |
| `Config` | value (key = config key) |
| `CustomEmoji` | name* |
| `Group`, `GroupChannel`, `GroupTeam`, `GroupMembership` | display_name/name/description/source/remote_id/timestamps/member_count + join tables |
| `CustomProfileField` / `CustomProfileAttribute` | field defs and per-user values |
| `PropertyField` / `PropertyValue` | generic property system (classifications etc.) |

(* = indexed)

**Relations** (`app/database/models/server/*.ts` `static associations`):
- `Channel` → has_many `ChannelBookmark`, `ChannelMembership`, `CategoryChannel`, `Draft`, `PostsInChannel`, `Post`, `MyChannel`(1:1), `ChannelInfo`(1:1); belongs_to `Team` (team_id), `User` (creator_id).
- `Post` → belongs_to `Channel`, `User`; has_many `Draft`(root_id), `File`, `PostsInThread`, `Reaction`, `Thread`(id, 1:1).
- `Team` → has_many `Category`, `Channel`, `MyTeam`(1:1), `TeamMembership`, `TeamSearchHistory`, `ThreadsInTeam`, `TeamChannelHistory`(1:1).
- `User` → has_many `Channel`(creator), `ChannelMembership`, `Post`, `Preference`, `Reaction`, `TeamMembership`, `ThreadParticipant`, `CustomProfileAttribute`.
- `Category` → has_many `CategoryChannel`; belongs_to `Team`.

**`System` table keys** (`app/constants/database.ts:60`, `SYSTEM_IDENTIFIERS`): `currentChannelId`, `currentTeamId`, `currentUserId`, `lastUnreadChannelId`, `license`, `dataRetentionPolicies`, `expandedLinks`, `teamHistory`, `recentReactions`, `recentCustomStatus`, `pushVerificationStatus`, `sessionExpiration`, and critically **`WebSocket`** which stores the **lastFullSync timestamp** (`app/queries/servers/system.ts` `getLastFullSync`/`setLastFullSync`). This single number drives all gap-filling.

### 1.2 Exact initial-sync sequence

Three distinct entries, all in `app/actions/remote/entry/`:

**A. `loginEntry` (`entry/login.ts`)** — after credentials are stored:
1. `fetchConfigAndLicense(serverUrl)` → `GET /api/v4/config/client?format=old` + `/license/client?format=old` (stored in `Config`/`System.license`).
2. Read stored credentials; register the server with SecurityManager/EphemeralModeManager.
3. `WebsocketManager.createClient(serverUrl, token, preauthSecret)` then `initializeClient(...)`.
   That's it — **login itself does not fetch teams/channels**. The websocket's first-connect callback does.

**B. `handleFirstConnect` / `handleReconnect` → `doReconnect` (`app/actions/websocket/index.ts:45-125`)** — the real sync, identical code for cold-connect and reconnect, differing only in the `since` value:

```
lastFullSync = System['WebSocket']            // 0 on first ever connect
entry(serverUrl, currentTeamId, currentChannelId, lastFullSync)
handleEntryAfterLoadNavigation(...)           // resolve kicked-from-team/channel
operator.batchRecords(models)                 // single write transaction
fetchPostDataIfNeeded(...)                    // posts for the visible channel/thread
deferredAppEntryActions(...)                  // background fan-out (see below)
setLastFullSync(now)
openAllUnreadChannels(); dataRetentionCleanup(); AppsManager.refreshAppBindings()
```

**`entry()` → `entryRest()` (`app/actions/remote/entry/common.ts:60-190`)**, exact ordering:

1. **Parallel**: `fetchConfigAndLicense()` + `fetchMyPreferences()` (`GET /users/me/preferences`).
2. Compute `isCRTEnabled` from prefs + `config.CollapsedThreads` / `FeatureFlagCollapsedThreads` (`processIsCRTEnabled`). **If CRT toggled since last run → `truncateCrtRelatedTables()` and force `lastDisconnectedAt = 0`** (full re-sync).
3. **Parallel**: `fetchMyTeams()` (`GET /users/me/teams` + `GET /users/me/teams/members`) and `fetchMe()` (`GET /users/me` + `/users/me/status` + roles).
4. Resolve `initialTeamId`: requested team → still a member? → else pick default via `selectDefaultTeam(myTeams, locale, teamsOrderPreference, config.ExperimentalPrimaryTeam)`.
   (Special case: if the stored channel was a GM that got converted to private, `fetchChannelById` and switch team.)
5. **Only for the initial team**: `fetchMyChannelsForTeam(serverUrl, initialTeamId, includeDeleted=false, since=lastDisconnectedAt, fetchOnly=true, excludeDirect=false, isCRTEnabled)`.
   Internally (`app/actions/remote/channel.ts`) this is **three parallel calls**:
   - `GET /users/me/teams/{team}/channels?include_deleted=false&last_delete_at={since}` (`client.getMyChannels`, `app/client/rest/channels.ts:281`)
   - `GET /users/me/teams/{team}/channels/members` (`client.getMyChannelMembers`)
   - `fetchCategories` → `GET /users/me/teams/{team}/channels/categories`
   Memberships are then filtered to channels present in the channel list.
6. `entryInitialChannelId(...)`: requested channel if still a member; DM/GM always OK; else walk `TeamChannelHistory.channel_ids` for the first channel you're still a member of (incl. the pseudo-IDs `GLOBAL_THREADS`/`GLOBAL_DRAFTS`); else the team default channel (`town-square`); else first open channel sorted by display name.
7. `prepareEntryModels(...)` builds all records; caller batches them in one write.

**`deferredAppEntryActions` (`entry/deferred.ts`)** — everything else, in a `setTimeout(0)`:
- `fetchMissingDirectChannelsInfo(...)` for DM/GM sidebar names (profile fetch by ID).
- `updateAllUsersSince(serverUrl, since)` (`GET /users?since=`), `updateCanJoinTeams()`.
- **Serially, team by team** (`teamQueue` = all teams except the initial one, ordered by the `teams_order` preference): `fetchMyChannelsForTeam(team.id, since)` → `processEntryModels(...)` per team.
- Then `processFinalInitializationTasks`: `fetchRoles(...)`, `processEntryModelsForDeletion(...)` (deletion is deliberately deferred until *all* teams have been fetched, otherwise you delete channels you were about to re-add), `syncTeamThreads(initialTeamId)` if CRT, **`fetchPostsForUnreadChannels(...)`**, `fetchTeamsThreads(...)` for other teams, `fetchGroupsForMember(...)`, `fetchScheduledPosts(initialTeamId)`.
- `autoUpdateTimezone(...)`.

**Post prefetch priority (`fetchPostsForUnreadChannels`, `app/actions/remote/post.ts:389`)**: filter memberships where `total_msg_count(_root) - msg_count(_root) > 0`; sort DM/GM (`team_id === ''`) first, then by team order, then by `last_viewed_at` desc; group by team; then `processChannelPostsByTeam` per team, batched.

**Per-channel post fetch (`fetchPostsForChannel`, `post.ts:290`)** — the key decision:
```
since = MyChannel.lastFetchedAt || newestPostInRecentChunk.create_at || 0
if since:  fetchPostsSince(channelId, since)  → actionType RECEIVED_SINCE
else:      fetchPosts(channelId, page=0, 60)  → actionType RECEIVED_IN_CHANNEL
```
There's a documented recovery path: a since-fetch is the only path that receives deletions; if applying them empties the newest `PostsInChannel` interval, the channel renders blank forever — so it destroys that interval and re-pages (`post.ts:328-347`, MM-66467). Same idea in `refreshPostsForChannel(isBlank)`.

Constants: `POST_CHUNK_SIZE = 60`, `POST_AROUND_CHUNK_SIZE = 10`, `PROFILE_CHUNK_SIZE = 100` (`app/constants/general.ts`).

**C. `pushNotificationEntry` (`entry/notification.ts`)** — a fast path that skips the full sync: set active server DB, load theme from prefs, `getMyTeamById`/`getMyChannel` from local DB and only `fetchMyTeam`/`fetchMyChannel` if missing, then either `fetchAndSwitchToThread(rootId)` (CRT) or `switchToChannelById(channelId, teamId)`, and *finally* `WebsocketManager.openAll()` (which triggers the full sync afterwards).

### 1.3 WebSocket reconnect gap-fill

Two layers.

**Transport (`app/client/websocket/index.ts`)** — mirror of the webapp client:
- `MAX_WEBSOCKET_FAILS = 7`, `MIN_WEBSOCKET_RETRY_TIME = 3s`, `MAX_WEBSOCKET_RETRY_TIME = 5min`; backoff `min(MIN * failCount, MAX)` once failCount > 7.
- **Reliable websockets** (`hasReliableWebsocket(version, config)`): URL becomes `wss://…/api/v4/websocket?connection_id={id}&sequence_number={serverSequence}`. On `hello`, if the returned `connection_id` differs from the stored one → server restart / too-long timeout / sequence lost → reset `serverSequence = 0` and run the **full sync** (`reconnectCallback`). If it matches, the server has replayed the buffered events and only `reliableReconnectCallback` fires (no REST sync at all).
- Any `msg.seq !== this.serverSequence` → log "missed websocket event", clear `connectionId`, `close(false)` and let the reconnect path re-sync.
- `WebsocketManager` (`app/managers/websocket_manager.ts`) layers connection lifecycle: `firstConnectionSynced` per server; staggered `openAll` (active server immediately, others every 5s); close-all on network loss or network *type* change (VPN↔wifi); a 15s background timer closes sockets when the app backgrounds; periodic status polling for all known user IDs.

**Application gap-fill (`doReconnect`)** — as listed in §1.2B. The essential mechanism: everything is driven by **one persisted timestamp** (`System['WebSocket']` = last successful full sync) plus **per-channel `MyChannel.lastFetchedAt`**:
- channels list: `?last_delete_at=since` gives you deletions since then;
- posts: `GET /channels/{id}/posts?since=` per channel — returns created *and updated/deleted* posts;
- users: `GET /users?since=`;
- threads: `GET /users/me/teams/{team}/threads?since=` (`syncTeamThreads`, `TeamThreadsSync.latest`).
- Only the *visible* channel/thread is fetched synchronously (`fetchPostDataIfNeeded`); everything else is prefetched by unread priority.

**Post chunk merging** (`app/database/operator/server_data_operator/handlers/post.ts`) — this is the part worth copying verbatim:
- `createPostsChain({order, posts, previousPostId})` (`app/database/operator/utils/post.ts:41`) links each post to `prev_post_id = order[i+1]`, last one to the API's `prev_post_id`, then reverses. This gives you a linked list you can walk without a global sort.
- `handlePostsInChannel` dispatches by action type:
  - `RECEIVED_IN_CHANNEL` → `handleReceivedPostsInChannel`: build `[earliest,latest]` **from non-deleted posts only**, find an overlapping chunk (`earliest ∈ [c.earliest,c.latest] || latest ∈ [...]`), extend it (`min`/`max`) or create a new one, then `mergePostInChannelChunks`.
  - `RECEIVED_SINCE` → only extends `chunks[0].latest` (chunks sorted `latest DESC`). Never creates a gap.
  - `RECEIVED_BEFORE` → only lowers `chunks[0].earliest` then merges.
  - `RECEIVED_NEW` (websocket) → extends `chunks[0].latest`.
  - `RECEIVED_AFTER` → **explicitly `throw new Error('Not implemented yet')`**. Mobile never paginates forward in a channel; it only jumps around a post (`fetchPostsAround`) or pages backwards.
- Reading a channel: `getRecentPostsInChannel` (`app/queries/servers/post.ts:195`) = take `chunks[0]` (highest `latest`) and `queryPostsChunk(channelId, earliest, latest)` → `create_at BETWEEN ? AND ? AND delete_at = 0 ORDER BY create_at DESC`. **Everything outside the newest chunk is invisible** until chunks merge. That's the whole gap model.
- `fetchPostsAround(channelId, postId)` (permalink): three parallel calls — `getPostsAfter(post,10)`, `getPostThread(post)`, `getPostsBefore(post,10)` — merged and stored as `RECEIVED_AROUND`.
- `handleReceivedPostsInThread` does the same interval math keyed on `root_id`.

### 1.4 Unreads / mentions

Counts live denormalized on `MyChannel`: `message_count`, `mentions_count`, `is_unread`, `last_viewed_at`, `viewed_at`, `manually_unread`.

- Server-truth path: `fetchMyChannelsForTeam` returns `ChannelMembership` with `msg_count`/`mention_count` (or `_root` variants for CRT) plus `Channel.total_msg_count(_root)`; `storeMyChannelsForTeam` computes the stored counts.
- Websocket `posted` (`app/actions/websocket/posts.ts:38-205`): if not `manuallyUnread` and the post is yours (non-system, non-webhook) → `markChannelAsViewed`; else if it's the current channel and the channel screen is mounted → don't mark unread; else `markChannelAsUnread(channelId, messageCount+1, mentionsCount + (msg.data.mentions.includes(me) ? 1 : 0), lastViewedAt)`. **The mention decision comes from `msg.data.mentions` on the websocket frame**, not from re-parsing the message.
- `markChannelAsUnread` (`app/actions/local/channel.ts`) sets `viewedAt = lastViewedAt - 1`, `manuallyUnread = true`, `isUnread = true`. `markChannelAsViewed` zeroes `mentionsCount`, clears `manuallyUnread`, sets `viewedAt = old lastViewedAt`, `lastViewedAt = now`, and removes OS notifications for the channel.
- Read receipt to server: `POST /channels/members/me/view {channel_id, prev_channel_id, collapsed_threads_supported: true}` (`app/client/rest/channels.ts:369`).
- Muting: a channel is muted when `MyChannelSettings.notify_props.mark_unread === 'mention'`. `isUnreadChannel(myChannel, notifyProps, lastUnreadChannelId)` = `mentionsCount > 0 || (!muted && isUnread) || id === lastUnreadChannelId` (`app/utils/categories.ts:25`). The `lastUnreadChannelId` trick keeps the channel you just opened visibly "unread-styled" until you leave.
- Aggregation: `observeMyChannelMentionCount` (`app/queries/servers/channel.ts:565`) sums `mentions_count` over MyChannel joined to Channel (`delete_at=0`, team filter) **and joined to MyChannelSettings with `notify_props NOT LIKE '%"mark_unread":"mention"%'`** — i.e. muted channels are excluded via a LIKE on the JSON blob. Team badge = channel mentions + thread mentions (`observeMentionCount`, `app/queries/servers/team.ts:417`).

### 1.5 Sidebar categories

- Server-side categories: `GET /users/me/teams/{team}/channels/categories` → `Category` (type ∈ `favorites` | `channels` | `direct_messages` | `custom`, `sorting` ∈ `alpha`|`recent`|`manual`, `muted`, `collapsed`, `sort_order`) + `CategoryChannel` join rows carrying per-channel `sort_order`. Enterprise "managed categories" are merged in client-side (`fetchCategories`, `app/actions/remote/category.ts:27`).
- Render pipeline (`app/utils/categories.ts`), per category:
  1. `filterArchivedChannels` (keep only `deleteAt === 0` or the currently-open channel)
  2. `filterManuallyClosedDms` — DMs visible only if `direct_channel_show/{teammateId}` pref is true (GMs: `group_channel_show/{channelId}`), unless unread
  3. `filterAutoclosedDMs` — only for the `direct_messages` category: keep unreads + current channel, drop DMs with deactivated users last-viewed before deactivation, drop never-opened DMs, sort by (current channel, unread, `max(lastViewedAt, direct_channel_show pref value)`), then slice to `max(limitPref, unreadCount)`
  4. `sortChannels(sorting, ...)` — `recent`: `max(myChannel.lastPostAt, channel.createAt)` desc; `manual`: `CategoryChannel.sort_order`; else alphabetical with **muted channels forced last**.
- Team ordering comes from the `teams_order` preference (comma-separated team IDs), unknown teams appended alphabetically (`observeSortedJoinedTeams`, `app/queries/servers/team.ts:440`).

### 1.6 Push notification flow

`app/init/push_notifications.ts` + `app/actions/remote/notifications.ts`.

- Registration: `Notifications.events().registerRemoteNotificationsRegistered` → device token stored globally → sent to the server via `client.setExtraSessionProps(deviceToken, notificationsDisabled, appVersion, voipToken)` from `setExtraSessionProps` (`entry/common.ts`) on every first-connect.
- Push proxy verification: `client.ping(deviceId)` returns `CanReceiveNotifications`; result stored in `System['pushVerificationStatus']` (`verifyPushProxy`).
- Three payload types (`payload.type`): `message`, `clear`, `session`.
  - **`clear`** → `handleClearNotification`: if CRT + `root_id` and thread is followed → `updateThread({unread_mentions:0, unread_replies:0, last_viewed_at:now})`; else `markChannelAsViewed(channelId)`. This is how read-state made on another device propagates.
  - **`session`** → emit `SESSION_EXPIRED` / `SERVER_LOGOUT`.
  - **`message`** → three sub-paths: foreground → in-app banner (`handleInAppNotification`, suppressed if you're already looking at that channel/thread); user tapped → `openNotification()` → `pushNotificationEntry`; background → `backgroundNotification()` which writes the post to the DB so the channel is warm when opened.
- **ID-loaded notifications**: when the payload is stripped for privacy, the native side fetches the post; on Android RN also re-fetches (`fetchNotificationData` → `fetchPostThread`/`fetchPostsForChannel`).
- Notifications are ACKed to the push proxy (`payload.ackId`, `verified` flag — unverified payloads are dropped).
- iOS-specific but instructive: the background completion handler is awaited until the DB write finishes, otherwise iOS suspends the app mid-SQLite-transaction and kills it (`0xdead10cc`).

### 1.7 WebSocket event table

`app/actions/websocket/event.ts` — ~100 cases. The set a minimal native client must handle: `posted`, `post_edited`, `post_deleted`, `post_unread`, `post_acknowledgement_added/removed`, `channel_created/deleted/unarchived/updated/converted/viewed`, `multiple_channels_viewed`, `channel_member_updated`, `direct_added`, `group_added`, `user_added/removed/updated`, `role_updated`, `memberrole_updated`, `preference_changed(s)/deleted`, `status_change`, `typing`, `reaction_added/removed`, `emoji_added`, `license_changed`, `config_changed`, `sidebar_category_created/updated/deleted/order_updated`, `thread_updated`, `thread_read_changed`, `thread_follow_changed`, `leave_team`/`added_to_team`/`update_team`/`delete_team`/`restore_team`.

---

## 2. webapp (`/tmp/mm/mattermost/webapp/channels`)

Note: `mattermost-redux` now lives at `webapp/channels/src/packages/mattermost-redux/`, not `webapp/platform/mattermost-redux` (that dir is just a package stub).

### 2.1 Redux entity store shape

`webapp/platform/types/src/store.ts` — `state.entities.{general, users, limits, teams, channels, channelBookmarks, posts, threads, recaps, agents, bots, preferences, admin, jobs, search, integrations, files, emojis, typing, roles, schemes, groups, channelCategories, apps, cloud, hostedCustomer, usage, scheduledPosts, sharedChannels, contentFlagging, properties}`, plus `state.requests`, `state.websocket`, `state.views`, `state.plugins`. Reducers combined in `packages/mattermost-redux/src/reducers/entities/index.ts`.

Key slices:

```ts
UsersState   { currentUserId, profiles: {id→UserProfile}, profilesInTeam/InChannel/InGroup: {id→Set<id>},
               profilesNotInTeam/…, statuses: {id→string}, lastActivity, dndEndTimes, mySessions, stats }
TeamsState   { currentTeamId, teams: {id→Team}, myMembers: {teamId→TeamMembership},
               membersInTeam: {teamId→{userId→TeamMembership}}, stats, totalCount }
ChannelsState{ currentChannelId, channels: {id→Channel}, channelsInTeam: {teamId→Set<id>},
               myMembers: {channelId→ChannelMembership}, roles: {channelId→Set<string>},
               membersInChannel: {channelId→{userId→ChannelMembership}}, stats,
               messageCounts: {channelId→{total, root}},   // ← separate from Channel
               manuallyUnread: {channelId→boolean}, channelsMemberCount, channelModerations, joinRequests }
PostsState   { posts: {id→Post}, postsReplies: {id→count},
               postsInChannel: {channelId→PostOrderBlock[]},
               postsInThread: {rootId→string[]},
               reactions: {postId→{userId+name→Reaction}},
               openGraph, pendingPostIds: string[], postEditHistory, currentFocusedPostId,
               messagesHistory, limitedViews: {channels:{id→ts}, threads:{rootId→ts}},
               acknowledgements: {postId→{userId→timestamp}} }
ThreadsState { threads: {id→UserThread}, threadsInTeam: {teamId→id[]}, unreadThreadsInTeam,
               counts / countsIncludingDirect: {teamId→{total, total_unread_threads, total_unread_mentions, total_unread_urgent_mentions}} }
FilesState   { files: {id→FileInfo}, fileIdsByPostId: {postId→id[]}, filesFromSearch, filePublicLink, rejectedFiles }
EmojisState  { customEmoji: {id→CustomEmoji}, nonExistentEmoji: Set<string> }
preferences  { myPreferences: {"category--name"→PreferenceType}, userPreferences: {userId→…} }
ChannelCategoriesState { byId: {id→ChannelCategory}, orderByTeam: {teamId→categoryId[]}, managedCategoryMappings }
```

Two things a native client should copy: **`messageCounts` is stored separately from `Channel`** (so a message-count update doesn't invalidate the channel object), and **preferences are keyed `${category}--${name}`** in a flat map.

### 2.2 Post indexing: `postsInChannel` blocks

```ts
type PostOrderBlock = { order: string[]; recent?: boolean; oldest?: boolean };
postsInChannel: Record<channelId, PostOrderBlock[]>
```
`order` is newest-first post IDs. `recent: true` = this block's newest post is the channel's newest post (you're at the bottom, live). `oldest: true` = this block reaches the beginning of the channel. **At most one block should be `recent`.** Multiple blocks = gaps.

Reducer `postsInChannel` (`packages/mattermost-redux/src/reducers/entities/posts.ts:645`):

- `RECEIVED_POSTS_IN_CHANNEL {recent, oldest}` — if `recent`, un-mark the current recent block; push the new block; `mergePostBlocks`. Short-circuits if the new order is identical to the existing recent block (same length, same first & last id).
- `RECEIVED_POSTS_BEFORE {beforePostId, oldest}` — pushes `{order: [beforePostId, ...order], recent:false, oldest}` — **the anchor post is deliberately included so the blocks overlap and merge**.
- `RECEIVED_POSTS_AFTER {afterPostId, recent}` — pushes `{order: [...order, afterPostId], recent}`, same trick.
- `RECEIVED_POSTS_SINCE` — only mutates the `recent` block: walks the incoming order backwards, skips posts older than the recent block's oldest post (those are *edits* to old posts, not new posts), skips already-present ids, unshifts the rest, re-sorts by `comparePosts`. Bails if there is no recent block ("shouldn't be dispatched if we haven't loaded the most recent posts yet").
- `RECEIVED_NEW_POST` (websocket) — unshifts into the recent block; if the channel has no blocks at all it does nothing ("don't save newly created posts until the channel has been loaded"); removes the matching `pending_post_id` and re-sorts (pending posts sort first).
- `RECEIVED_POST` — only acts when the post replaces a pending post in the recent block.
- CRT: both `RECEIVED_NEW_POST` and `RECEIVED_POST` **return early when `action.features.crtEnabled && post.root_id`** — replies never enter the channel feed under CRT.

`mergePostBlocks(blocks, posts)` (`posts.ts:1066`):
1. `removeNonRecentEmptyPostBlocks` — drop empty blocks except the recent one (an empty recent block is legal: a channel with only join/leave messages hidden).
2. sort blocks by `posts[block.order[0]].create_at` desc.
3. walk adjacent pairs: `a` ends at `posts[last(a.order)].create_at`, `b` starts at `posts[b.order[0]].create_at`; if `aEndsAt <= bStartsAt` they overlap → `mergePostOrder(a.order, b.order)` (append non-duplicates, re-sort desc by `create_at`), `recent = a.recent||b.recent`, `oldest = a.oldest||b.oldest`, splice out `b`, retry the same index.

**Which block gets rendered** (`components/post_view/post_list/index.tsx:60-90`) — this is the gap policy:
```
if (focusedPostId && post is loaded)      chunk = getPostsChunkAroundPost(state, focusedPostId, channelId)
else if (unreadChunkTimeStamp && !startFromBottom) chunk = getUnreadPostsChunk(state, channelId, ts)
else                                      chunk = getRecentPostsChunkInChannel(state, channelId)
postIds = chunk.order; atLatestPost = !!chunk.recent; atOldestPost = !!chunk.oldest
```
So the UI renders **exactly one block at a time**; a gap is never drawn — you simply switch which block you're looking at. `getUnreadPostsChunk` (`selectors/entities/posts.ts:512-565`) picks the recent chunk if its oldest post predates `lastViewedAt`, else the oldest chunk if appropriate, else searches for the chunk straddling the timestamp, else falls back to the recent chunk.

Pagination (`components/post_view/post_list/post_list.tsx`): `canLoadMorePosts(BEFORE_ID|AFTER_ID)` → if `!atOldestPost && type===BEFORE_ID` load 200 more before `getOldestPostId(postListIds)`, else if `!atLatestPost` load after the newest. `MAX_EXTRA_PAGES_LOADED = 30` guards channels full of hidden system messages; `AUTO_LOAD_POSTS_PER_PAGE = 200`; retries up to `MAX_NUMBER_OF_AUTO_RETRIES` then falls back to a manual "Load more" link.

Mount behaviour (`postsOnLoad`):
```
focusedPostId        → loadPostsAround(channelId, postId)   // getPostsAround, ±POST_CHUNK_SIZE/2
isFirstLoad          → loadUnreads(channelId)               // unless prefetch already running
latestPostTimeStamp  → syncPostsInChannel(channelId, latestPostTimeStamp)
else                 → loadLatestPosts(channelId)
then (if not permalink) markChannelAsRead(channelId)
```
`loadUnreads` → `GET /users/{me}/channels/{id}/posts/unread?limit_before=30&limit_after=30`; `recent = next_post_id === ''`, `oldest = prev_post_id === ''`. If `next_post_id !== ''` and the user's `unread_scroll_position` preference is "newest", it *also* fetches `getPosts(page 0)` and dispatches a second block — creating a deliberate gap between the unread block and the bottom block.

`syncPostsInChannel(channelId, since)` (`actions/views/channel.ts:433`) uses `min(since, lastPostsApiTimeForChannel)` when the last API call predates `websocket.lastDisconnectAt` — so a channel that was loaded before you went offline gets synced from *its* last fetch, not from the socket disconnect time.

`postsInThread: {rootId → string[]}` (unordered id list, `posts.ts:1147`) is populated from `RECEIVED_POSTS_*` for any post with `root_id`, and from `RECEIVED_POSTS_IN_THREAD`. There is no block/gap machinery for threads in the webapp — `getPostThread` returns the whole thread (paginated variants exist via `getPaginatedPostThread`).

Post list rendering inserts virtual rows (`packages/mattermost-redux/src/utils/post_list.ts`): `date-{timestamp}`, `start-of-new-messages-{timestamp}`, `create-comment`, `user-activity-{id1_id2…}` (combined join/leave, `MAX_COMBINED_SYSTEM_POSTS = 100`). `makePreparePostIdsForPostList` = `filterPostsAndAddSeparators` ∘ `combineUserActivityPosts`.

### 2.3 Layout — how many columns actually exist

**Five** vertical regions, laid out as a CSS grid in `webapp/channels/src/sass/base/_structure.scss:112-200`:

```
#root grid-template:
  "classification-banner ×3" / "announcement ×3" / "admin-announcement ×3"
  "header header header"                       ← <GlobalHeader>
  "team-sidebar   main   app-sidebar"          ← columns: min-content | minmax(385px,1fr) | min-content
  "classification-banner-bottom ×3" / "footer ×3"

.main-wrapper grid-template: "lhs center rhs"  ← min-content | minmax(385px,1fr) | min-content
```

Component tree (`components/root/root.tsx:420-485`):
- `<GlobalHeader>` — top bar, full width (`components/global_header/`, with `left_controls`/`center_controls`(search)/`right_controls`).
- `<TeamSidebar>` — grid-area `team-sidebar`, the narrow team rail (`components/team_sidebar/`); **only rendered when the user is in >1 team** (or the multi-team setting is on).
- `.main-wrapper` → `<TeamController>` → `<ChannelController>` (`components/channel_layout/channel_controller.tsx`) renders:
  - `<Sidebar/>` → `#SidebarContainer`, grid-area `lhs` — the channel sidebar (`components/sidebar/sidebar.tsx`: `SidebarHeader` + `ChannelNavigator` + `SidebarList` of `SidebarCategory`→`SidebarChannel`, plus `unread_channel_indicator` and `channel_filter`).
  - `#channel_view` → grid-area `center` → `<CenterChannel>` (`channel_layout/center_channel/center_channel.tsx`), which is a `<Switch>` over `/pl/:postid` (PermalinkView), `/:team/(channels|messages)/:identifier/:postid?` (ChannelIdentifierRouter → ChannelView), `/:team/threads/:id?` (GlobalThreads), `/:team/drafts`, `/:team/recaps`.
- `<SidebarRight/>` — grid-area `rhs`, rendered as a sibling *outside* `ChannelController`, inside `.main-wrapper` (`root.tsx:478`). Holds thread view / search results / pinned / files / channel info / plugin RHS.
- `<AppBar/>` — grid-area `app-sidebar`, the far-right plugin icon rail (`components/app_bar/app_bar.tsx`), conditional on `shouldShowAppBar`.

So: **team rail | channel sidebar | center | RHS | app bar** = up to 5 columns.

**Responsive/collapse** (`_structure.scss:243-310`):
- `≤768px` (mobile): the whole grid collapses to a single `"main"` area — `team-sidebar`, `app-bar`, `#SidebarContainer`, `#channel_view`, `#sidebar-right` all get `grid-area: main` and overlay each other; the sidebar slides in via `.move--right`/`.move--left` classes on the inner wrap (see `center_channel.tsx:80`, `lhsOpen`/`rhsOpen`/`rhsMenuOpen`), and `<MobileSidebarRight>` is mounted instead. Border radii and the 4px window inset are removed.
- `768px–1200px` with RHS expanded (`.rhs-open-expanded`): the RHS width-holder is `display:none` and `#sidebar-right` takes over `grid-area: center` at `width:100%` — i.e. the RHS *replaces* the center channel rather than squeezing it.
- The `min-content` columns mean the sidebars are sized by their own content/CSS var; the center is `minmax(385px, 1fr)`.

**Resizable panels** (`components/resizable_sidebar/`):
- `ResizableLhs`/`ResizableRhs` wrap the panel and render a `ResizableDivider` that writes a CSS custom property (`--overrideLhsWidth` / `--overrideRhsWidth`) and persists the value.
- `DEFAULT_LHS_WIDTH = 240`. RHS min/max depend on a `SidebarSize` derived from viewport width (`constants.ts`): SMALL 400/400, MEDIUM 304–400, LARGE 304–464, XLARGE 304–776 (default 400, 500 at XLARGE).
- `SIDEBAR_SNAP_SIZE = 16`, `SIDEBAR_SNAP_SPEED_LIMIT = 5` — snap-to-edge behaviour when dragging fast.

For GTK4: `AdwOverlaySplitView` (collapsible LHS) nested with a second split view or `AdwBreakpoint`-driven `GtkPaned` for the RHS maps onto this cleanly; the ≤768px behaviour is exactly `AdwOverlaySplitView` collapsed mode.

### 2.4 Webapp startup + reconnect (for contrast with mobile)

- `loadMe()` (`packages/mattermost-redux/src/actions/users.ts:70`) — **parallel**: `getClientConfig`, `getLicenseConfig`, `getMe`, `getMyPreferences`, `getMyTeams`, `getMyTeamMembers`; then `getMyTeamUnreads(crt)`, then `getServerLimits()`.
- Per team on navigation: `initializeTeam(team)` (`components/team_controller/actions/index.ts:25`) → `selectTeam`, status poll, groups; and `fetchChannelsAndMembers(teamId)` = parallel `getMyChannels(teamId)` + `getMyChannelMembers(teamId)`, then `loadRolesIfNeeded(roles from members)`.
- `DataPrefetch` (`components/data_prefetch/data_prefetch.tsx`): a priority queue that fetches **2 channels at a time**, mentions first then unreads; re-built whenever the unreads selector changes; adds 0–1000 ms jitter for public/private channels whose last post is <1 s old to avoid a thundering herd; `prefetchChannelPosts` → `loadUnreads(prefetch)` if never loaded, else `syncPostsInChannel(recentPost.create_at, prefetch)`.
- `reconnect()` (`actions/websocket_actions.ts:275`): `fetchAllMyTeamsChannels()`, `fetchAllMyChannelMembers()`, `fetchMyCategories(currentTeamId)`, `loadProfilesForSidebar()`, then `syncPostsInChannel(currentChannelId, mostRecentPost.create_at)` (or `getPosts` if the channel never loaded), `getMyTeamUnreads(crt)`, `syncThreads(teamId)` per team (`getCountsAndThreadsSince(userId, teamId, newestThread.last_reply_at)`), `checkForModifiedUsers()` if `lastDisconnectAt`, `WebSocketClient.updateActiveChannel/updateActiveTeam`.

**Unread computation** (`packages/mattermost-redux/src/utils/channel_utils.ts:371`):
```ts
crtEnabled ? (messages = messageCount.root - member.msg_count_root, mentions = member.mention_count_root)
           : (messages = messageCount.total - member.msg_count,     mentions = member.mention_count)
showUnread = mentions > 0 || (!isChannelMuted(member) && messages > 0)
hasUrgent  = member.urgent_mention_count > 0
```
`isChannelMuted(member)` = `member.notify_props.mark_unread === 'mention'`.

**Permalink** (`components/permalink_view/actions.ts` `focusPost`): `GET /posts/{id}/info` → if `!has_joined_channel`, prompt (private) and `joinChannel` → `getPostThread(postId)` → resolve channel (fetch if unknown) → resolve/`joinChannel` membership → select team+channel → dispatch `RECEIVED_FOCUSED_POST` and route. Errors route to `/error?type=permalink_not_found` or `cloud_archived` (`first_inaccessible_post_time`).

---

## 3. desktop

**It is an Electron shell around the *hosted, server-served* webapp** — it does not bundle the webapp. Each server is loaded in a `WebContentsView` pointed at the real server URL (`src/app/views/MattermostWebContentsView.ts`, managed by `WebContentsManager`). The desktop's own React code (`src/renderer/`) only renders chrome: tab bar, dropdowns, modals (add server, settings), loading screen, downloads dropdown — separate webpack entry points served from a custom `mattermost-desktop://` protocol.

Entry: `src/main/app/index.ts` → `initialize()` in `src/main/app/initialize.ts`.

**Native features it adds:**

| Feature | Where |
|---|---|
| **Multi-server + tabs** | `common/servers/serverManager.ts`, `common/views/viewManager.ts`, `src/app/tabs/tabManager.ts`. Each server has multiple *views* (`ViewType.TAB` vs `ViewType.WINDOW`); tab order persisted, `SWITCH_TAB`/`TAB_ADDED`/`TAB_REMOVED`/`TAB_ORDER_UPDATED` IPC. `Cmd/Ctrl+1..9` switches servers (`app/menus/appMenu/window.ts:52`). |
| **Popout windows** | `src/app/windows/popoutManager.ts` + `src/app/windows/baseWindow.ts` — RHS/thread/channel popped out into isolated `BaseWindow`s, message-passed via `sendToParent`/`sendToPopout`. |
| **Tray icon** | `src/app/system/tray/tray.ts` — three states (`normal`/`unread`/`mention`) with per-platform assets (Windows `.ico` light/dark, macOS template image, Linux `top_bar_{dark,light}_{unread,mention}_16.png`); `update(status, tooltip)`; session-expired shows the mention icon. Tray context menu built by `MenuManager` (`app/menus/tray.ts`). |
| **Unread badges** | `src/app/system/badge.ts`. Windows: draws a red circle + count on a canvas **inside the main window's web contents** (no canvas in the main process) → `setOverlayIcon`, capped at `99+`. macOS: `app.dock.setBadge(count \|\| '•' \|\| '!')`. Linux: `app.setBadgeCount(mentions + expired)` **only if `app.isUnityRunning()`**. Fed by `AppState` (`src/common/appState.ts`) which keeps `mentions`/`unreads`/`expired` per view, reduces to per-server totals, and emits `UPDATE_APPSTATE_TOTALS`. |
| **Notifications** | `src/main/notifications/index.ts`. `NotificationManager.displayMention(title, body, channelId, teamId, url, silent, webcontents, soundName)` — checks `Notification.isSupported()`, per-platform DND (`macos-notification-state`, `windows-focus-assist`, none on Linux), `viewManager.isPrimaryView()`, `PermissionsManager.doPermissionRequest('notifications')`; dedupes per channel via `mentionsPerChannel`; on click: `MainWindow.show()` + `TabManager.switchToTab(view.id)` **deferred until the next `BROWSER_HISTORY_PUSH`** so focus lands after navigation, then sends `NOTIFICATION_CLICKED(channelId, teamId, url)` back into the webapp. Sounds from `src/assets/sounds/`. |
| **Deep links** | `mattermost://` registered via `app.setAsDefaultProtocolClient(MATTERMOST_PROTOCOL)` (`initialize.ts:235`); macOS `app.on('open-url')` (`main/app/app.ts:71`), other platforms `app.on('second-instance')`. Queued in `NavigationManager.queuedDeepLink` until ready; resolved to a server via `ServerManager.lookupServerByURL`, then converted to a `browserHistory.push` inside the view (server ≥6.0.0) instead of a full page load. |
| **Browser-history bridge** | `BROWSER_HISTORY_PUSH` / `REQUEST_BROWSER_HISTORY_STATUS` — the webapp's router state drives the app's back/forward menu items. |
| **Calls widget window** | `src/app/callsWidgetWindow.ts` — a **separate frameless, transparent, always-on-top, non-resizable `BrowserWindow`** (`backgroundColor:'#00ffffff'`, `hasShadow:false`, preload `externalAPI.js`) loading the calls plugin widget URL with `call_id`/`title`/`root_id` query params. Resized on demand via `CALLS_WIDGET_RESIZE` (min `MINIMUM_CALLS_WIDGET_WIDTH/HEIGHT`). Handles `CALLS_WIDGET_SHARE_SCREEN`, `CALLS_WIDGET_OPEN_THREAD`, `..._OPEN_STOP_RECORDING_MODAL`, `..._OPEN_USER_SETTINGS` by forwarding to the main app view. Navigation is locked down (`will-navigate`/`did-start-navigation`/`will-redirect` all blocked). |
| **Screen-share picker** | `handleGetDesktopSources` in `callsWidgetWindow.ts:495-560`: on macOS, if `systemPreferences.getMediaAccessStatus('screen') === 'denied'` it resets TCC permissions and opens System Settings; then `PermissionsManager.doPermissionRequest('screenShare')`; then `desktopCapturer.getSources(opts)` mapped to `{id, name, thumbnailURL: thumbnail.toDataURL()}`. Failures emit `CALLS_ERROR('screen-permissions', callID)`. |
| **Shortcuts** | Not `globalShortcut` — an application `Menu` with accelerators built per-menu in `src/app/menus/appMenu/{file,edit,view,history,window,help}.ts` and rebuilt on config/tab/view change. `Cmd+F` search, `Cmd+R`/`Shift+Cmd+R` reload, `Cmd+0/=/-` zoom, `Cmd+T` new tab, `Cmd+W`/`Shift+Cmd+W` close tab/window, `Cmd+N` new window, `Cmd+,` settings, `Cmd+1..9` server switch. |
| Other | `DownloadsManager`, `updateNotifier` (fetches a remote manifest), `AutoLauncher`, `ThemeManager` (syncs server theme ↔ OS theme), `UserActivityMonitor` (`powerMonitor.getSystemIdleTime()` → away status), `SecureStorage` (`safeStorage`), `CertificateStore`, `PermissionsManager`, `preAuthManager`, `performanceMonitor` (pushes CPU/mem into the server view for Prometheus). |

**The contract between webapp and shell** is `window.desktopAPI` (`/tmp/mm/desktop/api-types/index.ts`, consumed at `webapp/channels/src/utils/desktop_api.ts`). The ones a native client must reimplement natively rather than proxy: `sendNotification(title, body, channelId, teamId, url, silent, soundName)`, `onNotificationClicked`, **`setUnreadsAndMentions(isUnread, mentionCount)`**, `setSessionExpired`, `updateTheme`, `getDarkMode`/`onDarkModeChanged`, `getDesktopSources`, `shareScreen`, `openPopout`. Older servers fall back to `window.postMessage({type:'dispatch-notification'|'browser-history-push'})`.

---

## 4. `@mattermost/client` (`webapp/platform/client`)

### 4.1 `Client4` (`src/client4.ts`, 5490 lines)

Public mutable state: `token`, `csrf`, `url`, `urlVersion = '/api/v4'`, `userAgent`, `defaultHeaders`, `userId`, `serverVersion`, `clusterId`, `includeCookies = true`, `setAuthHeader = true`, `diagnosticId`, `userRoles`.

**Route groups** (`getXRoute()` helpers, `client4.ts:276-560`) — a good checklist of the API surface: users, teams (+ members, scheme, name), channels (+ members, scheme, bookmarks, join requests, categories), posts, reactions, commands, files, preferences, incoming/outgoing hooks, shared channels, oauth (+ apps, outgoing connections), emojis, brand, data retention, jobs, recaps, plugins (+ marketplace), agents/LLM services, roles, schemes, bots, groups, notices, cloud/hosted customer/usage, permissions, user threads, system, remote clusters, property fields/values, custom profile attributes, apps proxy.

**Auth / headers** — `getOptions(options)` (`client4.ts:593`):
```
X-Requested-With: XMLHttpRequest                     // always; server uses this as CSRF defence for cookie auth
Authorization: BEARER <token>                        // iff setAuthHeader && token
X-CSRF-Token: <csrf || MMCSRF cookie value>          // iff method !== 'get' and a token exists
credentials: 'include'                               // iff includeCookies
Content-Type: application/json                       // unless body is FormData
```
So it's **both**: browsers get `MMAUTHTOKEN` + `MMCSRF` cookies and the `X-CSRF-Token`/`X-Requested-With` pair; non-browser clients set `setAuthHeader` and send `Authorization: Bearer`. `getCSRFFromCookie()` (`client4.ts:580`) scrapes `document.cookie` for `MMCSRF=`. **For a Rust client: set `Authorization: Bearer <token>` + `X-Requested-With: XMLHttpRequest`, skip cookies and CSRF entirely.**

**Token acquisition**: `login()` does `POST /users/login {login_id, password, token, deviceId}` and reads the token from the **`Token` response header**, then `setToken()`. Variants: `loginById`, `loginWithDesktopToken` (`POST /users/login/desktop_token`), `loginWithMagicLink` (`/users/login/one_time_link`), `getUserLoginType`. `logout()` clears `token` and `serverVersion`.

**`doFetchWithResponse`** (`client4.ts:4873`): `fetch(url, getOptions(options))` → `parseAndMergeNestedHeaders` → body parsed by `Content-Type` (`application/json`, `application/x-ndjson` line-delimited, `application/zip`/`text/csv` as blob, else text) → caches `X-Version-Id` into `serverVersion` (only when no `Cache-Control`) and `X-Cluster-Id` into `clusterId` → on `!response.ok` throws `ClientError{message, server_error_id, status_code, detailed_error, url}` (`src/errors.ts`).

**Post endpoints** (the ones that matter, `client4.ts:2631-2687`):
```
GET /channels/{id}/posts?page&per_page&skipFetchThreads&collapsedThreads&collapsedThreadsExtended
GET /users/{me}/channels/{id}/posts/unread?limit_after=30&limit_before=30&…
GET /channels/{id}/posts?since=…
GET /channels/{id}/posts?before={postId}&page&per_page
GET /channels/{id}/posts?after={postId}&page&per_page
GET /posts/{id}/thread  (getPaginatedPostThread: direction, fromPost, fromCreateAt, perPage, fetchAll)
POST /users/{userId}/posts/{postId}/ack     /  DELETE  (acknowledgePost/unacknowledgePost)
POST /users/me/drafts (with `Connection-Id` header)   /  GET /users/me/teams/{team}/drafts
```
All post-list responses are `{order: string[], posts: {id→Post}, next_post_id, prev_post_id, first_inaccessible_post_time}` — `next_post_id === ''` ⇒ you're at the newest (`recent`), `prev_post_id === ''` ⇒ at the oldest (`oldest`).

### 4.2 `websocket.ts` (`src/websocket.ts`, 678 lines)

Config defaults:
```ts
maxWebSocketFails: 7, minWebSocketRetryTime: 3000, maxWebSocketRetryTime: 300000,
reconnectJitterRange: 2000, clientPingInterval: 30000
```
Custom close codes: `4000` client-ping-timeout, `4001` client-sequence-mismatch.

**Connect** — `initialize(connectionUrl, token?, postedAck?)`:
- returns early if `conn` exists **or a `reconnectTimeout` is pending** (so backoff is never short-circuited);
- URL: `${connectionUrl}?connection_id=${connectionId}&sequence_number=${serverSequence}` (+`&posted_ack=true`, +`&disconnect_err_code=…` reporting the previous close code);
- registers `window` `online`/`offline` handlers: online → schedule reconnect after `minWebSocketRetryTime`; offline → send an immediate ping to test the socket rather than assuming death;
- `onopen`: if a `token` was passed, send `authentication_challenge {token}` (that's how non-cookie clients authenticate); fire `reconnect` listeners if `connectFailCount > 0`, else `firstConnect`; start the 30 s ping loop; reset `connectFailCount = 0`.

**Backoff** — `onclose`:
```
connectFailCount++
retryTime = minWebSocketRetryTime
if connectFailCount > maxWebSocketFails:
    retryTime = min(minWebSocketRetryTime * failCount * failCount, maxWebSocketRetryTime)   // quadratic
retryTime += random() * reconnectJitterRange
```
(Note the webapp uses `min * n²` while mobile uses `min * n` — mobile is gentler.) If a `reconnectTimeout` already exists it defers to it.

**Ping/pong liveness**: every 30 s, if the previous ping hasn't been answered → stop the interval, synthesize a `CloseEvent(code 4000)`, detach `conn.onclose`, `conn.close()`, and call `onclose` manually (because Electron/browsers can delay the real close event for a long time).

**Reliable websockets / gap detection** — `onmessage`:
- `msg.seq_reply` present → it's a response to a request, resolve `responseCallbacks[seq_reply]`, **skip sequence validation**.
- `msg.event === 'hello'` → read `data.connection_id` and `data.server_hostname`. If a previous `connectionId` existed and differs → **the server could not replay**: fire `missedMessageListeners` (the webapp's full-resync hook) and reset `serverSequence = 0`.
- Then, for every event: `if (msg.seq !== serverSequence)` → log `missed websocket event, act_seq/exp_seq`, synthesize `CloseEvent(4001)`, force-close and reconnect. Otherwise `serverSequence = msg.seq + 1` and dispatch to listeners.

**Outbound messages** (`sendMessage(action, data, cb)` with `seq: responseSequence++`): `authentication_challenge`, `user_typing {channel_id, parent_id}`, `presence {channel_id|team_id|thread_id}` (`updateActiveChannel`/`updateActiveTeam`/`updateActiveThread`), `user_update_active_status`, `posted_notify_ack {post_id, user_agent, status, reason, post}` (the delivery receipt enabled by `posted_ack=true`), `get_statuses`, `get_statuses_by_ids`.

Listener API is the modern `addMessageListener`/`addFirstConnectListener`/`addReconnectListener`/`addMissedMessageListener`/`addErrorListener`/`addCloseListener` (the `setXCallback` forms are deprecated); it warns above 5 listeners of a kind.

---

## 5. Distilled recommendations for a native GTK4/Rust client

### 5.1 Minimal correct startup sequence

```
0. token = POST /api/v4/users/login  → read `Token` response header (or use a PAT)
   headers thereafter: Authorization: Bearer <token>, X-Requested-With: XMLHttpRequest

1. parallel:  GET /config/client?format=old
              GET /license/client?format=old
              GET /users/me/preferences
   → derive crt_enabled = processIsCRTEnabled(prefs, config.CollapsedThreads,
                                              config.FeatureFlagCollapsedThreads, config.Version)
   → if crt_enabled changed since last run: wipe Thread/ThreadParticipant/ThreadsInTeam/
     TeamThreadsSync tables and force since = 0

2. parallel:  GET /users/me            (+ /users/me/status)
              GET /users/me/teams
              GET /users/me/teams/members
   → pick initial team: last used → still a member? else teams_order pref → else
     ExperimentalPrimaryTeam → else alphabetical

3. for the initial team only, in parallel:
              GET /users/me/teams/{team}/channels?include_deleted=false&last_delete_at={since}
              GET /users/me/teams/{team}/channels/members
              GET /users/me/teams/{team}/channels/categories
   → pick initial channel: requested → team channel history (MRU) → default channel → first open channel

4. open the websocket:  wss://…/api/v4/websocket?connection_id=&sequence_number=0
   send authentication_challenge {token}; store connection_id from `hello`

5. render the shell from local data NOW.  Then, for the visible channel:
   since = my_channel.last_fetched_at (or newest cached post's create_at)
   since ? GET /channels/{id}/posts?since={since}
         : GET /users/me/channels/{id}/posts/unread?limit_before=30&limit_after=30
   fetch missing post authors; POST /channels/members/me/view

6. deferred (background task, do not block the UI):
   - profiles for DM/GM sidebar names
   - GET /users?since={since}
   - per remaining team (serially): channels + members + categories
   - GET /users/me/teams/{team}/threads (if CRT)
   - unread channels' posts, priority: DMs/GMs → team order → last_viewed_at desc, ~2 concurrent
   - roles for all membership role names; groups; scheduled posts
   - only after all teams are fetched: delete local teams/channels no longer present

7. persist last_full_sync = now (a single scalar, like mobile's System['WebSocket'])
```
Do **not** copy the webapp's habit of loading everything eagerly; copy mobile's "render from SQLite, sync in the background" shape.

### 5.2 What to cache locally, in what shape

SQLite. Take mobile's schema almost verbatim (§1.1) — it is a battle-tested normalization of the API:

- **`channel` vs `my_channel` split.** Keep the channel object separate from your membership row. Your membership carries the mutable hot data (`message_count`, `mentions_count`, `is_unread`, `last_viewed_at`, `viewed_at`, `manually_unread`, `roles`, `last_fetched_at`) and is written on nearly every websocket event; the channel object is written rarely. Same for `team`/`my_team`, and put channel purpose/header/counters in a third `channel_info` row so header edits don't churn the channel row.
- **`post` with `previous_post_id`.** Store the server's ordering link, not just `create_at` — it survives identical timestamps and lets you detect a broken chain.
- **`posts_in_channel(channel_id, earliest, latest)` and `posts_in_thread(root_id, earliest, latest)`.** This is the single most important table. Sort by `latest DESC`; the channel feed is `SELECT * FROM post WHERE channel_id=? AND create_at BETWEEN chunk.earliest AND chunk.latest AND delete_at=0 ORDER BY create_at DESC`.
- **`preference(category, name, user_id, value)`** — index on `(category, user_id)`; you'll read `direct_channel_show`, `group_channel_show`, `teams_order`, `display_settings`, `theme`, `limit_visible_dms_gms`, `collapsed_reply_threads`, `unread_scroll_position` constantly.
- **`system(id, value)`** key/value scalars: `current_team_id`, `current_channel_id`, `current_user_id`, `last_full_sync`, `license`, `team_history`, `last_unread_channel_id`.
- **`config(key, value)`** as its own table — the client config is ~300 keys and is read on every feature check.
- JSON-blob columns are fine for `notify_props`, `props`, `metadata`, `timezone` — but note mobile pays for this with `Q.where('notify_props', Q.notLike('%"mark_unread":"mention"%'))`. **Promote `muted` to a real boolean column** and you'll thank yourself.
- Files: store `FileInfo` rows + a `local_path` for downloaded originals and a small `image_thumbnail` (base64 mini-preview) inline, like mobile.
- Drafts and scheduled posts locally (drafts sync via `POST /users/me/drafts` with a `Connection-Id` header so your own websocket echo can be ignored).

Reactivity: mobile gets live UI from WatermelonDB observables. In Rust, the equivalent is a change-notification bus keyed by (table, row id) that your GTK widgets subscribe to — write through one "operator" layer that batches all records for an event into a single transaction and then emits the invalidations, exactly like `operator.batchRecords(models, 'label')`.

### 5.3 Known hard parts

**a) Post ordering and gaps.** Two viable models, don't mix them:
- *Interval model* (mobile): `(earliest, latest)` per channel, render only the newest interval, merge on overlap. Simple, but you must never let a deleted post define an interval (MM-66467: a deleted post newer than everything else creates an empty interval that shadows the whole channel) and you must handle "the newest interval became empty" by destroying it and re-paging. `RECEIVED_AFTER` is unimplemented on mobile — if you want forward pagination you have to write the merge yourself.
- *Block model* (webapp): ordered ID lists with `recent`/`oldest` flags, merged by `mergePostBlocks`. Render exactly one block; switch blocks for unread-jump and permalink. Always include the anchor post in `before`/`after` responses so blocks overlap and merge.
Either way: `recent` means "`next_post_id === ''`", `oldest` means "`prev_post_id === ''`", and posts arriving from `?since=` that are older than your newest block are **edits, not new posts** — dropping that check produces duplicated/misordered feeds.

**b) Permalink jumps.** `GET /posts/{id}/info` first (it tells you `has_joined_channel`, `channel_type`, `channel_display_name` without requiring membership); you may need to join a public/private channel before you can render. Then fetch around the post (`before` + `thread` + `after`, 10 each on mobile / `POST_CHUNK_SIZE/2` on web) as an isolated block, and be ready for `first_inaccessible_post_time` (cloud retention) and for the target post to be a *reply*, which under CRT means opening the thread pane instead of scrolling the channel.

**c) CRT (collapsed reply threads).** This is a mode switch, not a view option:
- Enabled = `processIsCRTEnabled(preferences, config.CollapsedThreads, config.FeatureFlagCollapsedThreads, config.Version)` — a per-user preference gated by a server setting that can be `disabled`/`default_on`/`default_off`/`always_on`.
- When it flips, **you must truncate all thread state and force a full re-sync** (`truncateCrtRelatedTables`, `lastDisconnectedAt = 0`). Mobile does exactly this.
- Under CRT: replies (`root_id != ''`) must NOT enter the channel feed (both webapp reducers return early); unread math switches to the `_root` counters (`total_msg_count_root - msg_count_root`, `mention_count_root`); every fetch carries `collapsedThreads=true&collapsedThreadsExtended=true`; and you get a whole parallel entity — `Thread` (`reply_count`, `unread_replies`, `unread_mentions`, `last_reply_at`, `last_viewed_at`, `is_following`) plus `ThreadsInTeam` and a per-team `(earliest, latest)` sync watermark.
- Thread sync (`syncTeamThreads`, `app/actions/remote/thread.ts`): first time → two calls, *all unread threads* + *the latest page*; afterwards → *all unread* + *all threads since `latest+1` with `deleted=true`*. The unread call is repeated every time because unread state changes don't bump `last_reply_at`.
- Websocket: `thread_updated`, `thread_read_changed`, `thread_follow_changed`.

**d) Channel member sync.** `GET /users/me/teams/{t}/channels/members` returns *your* memberships only; other members are lazy (`GET /channels/{id}/members?page`). Watch for:
- `?last_delete_at={since}` on `getMyChannels` returns channels deleted since then — you must reconcile deletions, and **only after fetching every team**, otherwise you delete channels you're about to re-add (see `processEntryModelsForDeletion` being deferred in `entry/deferred.ts`).
- Being removed while offline: `handleKickFromTeam`/`handleKickFromChannel` and the `handleEntryAfterLoadNavigation` state machine (`entry/common.ts:340-400`) — the user may have navigated during the sync, so you must compare current team/channel *after* load against what you started with.
- GM→private-channel conversion moves a channel between teams and changes its type; mobile special-cases this on every entry.
- `channel_member_updated`, `memberrole_updated`, `user_added`/`user_removed`, `direct_added`/`group_added` all mutate membership; `multiple_channels_viewed` mutates read state for many channels at once (another device marking things read).

**e) Unread/mention computation.** The exact formula (§2.4). Traps: use the `_root` counters under CRT; a *muted* channel still shows mentions but not message-unreads; `manually_unread` must survive websocket posts (mobile guards every `markChannelAsViewed` on it); mentions come from `msg.data.mentions` on the `posted` frame, not from parsing text; `last_unread_channel_id` keeps the just-opened channel styled as unread; team badge = channel mentions + thread mentions; the desktop shell's badge/tray takes only `(isUnread: bool, mentionCount: int)` — that's the right granularity for your GTK notification/badge layer too.

**f) Message priority & acknowledgements.** `post.metadata.priority = {priority: 'important'|'urgent'|'', requested_ack?: bool, persistent_notifications?: bool}`; gated by `config.PostPriority` / `PostAcknowledgements` / `PersistentNotifications` + license. Acks: `POST/DELETE /users/{userId}/posts/{postId}/ack`, stored as `acknowledgements: {postId → {userId → timestamp}}` in the webapp; websocket `post_acknowledgement_added`/`removed`. `urgent_mention_count` on the channel member drives the "urgent" badge variant. Persistent notifications re-notify on an interval until acknowledged — if you implement it, respect `config.PersistentNotificationIntervalMinutes` and stop on ack/reply.

**g) Reliable-websocket resync, precisely.** Store `connection_id` and `serverSequence`. On reconnect send both in the query string. Three outcomes: (i) `hello.connection_id` equals yours → the server replayed the buffer, **do no REST sync at all**; (ii) it differs → full sync with `since = last_full_sync`; (iii) any `msg.seq != expected` mid-stream → drop the connection (close code 4001) and take path (ii). Also implement the 30 s ping with manual `onclose` synthesis — real close events can arrive minutes late and your UI will show "connected" while dead. Backoff: 3 s flat for the first 7 failures, then quadratic (web) or linear (mobile) up to 5 min, plus ≤2 s jitter. And close the socket when the network *type* changes (VPN↔wifi), not just when it drops.

**h) One more, unstated but real:** every list endpoint the clients use is `GET` with a `since`/`before`/`after`/`last_delete_at` watermark, and every one of them can return *updated* and *deleted* records interleaved with new ones. Your persistence layer needs a single `upsert_or_delete(records)` path per entity (mobile's `processRecords` with a `shouldUpdate: n.update_at > e.update_at` predicate) rather than per-call-site handling, or you will get ghost rows.
agentId: a44b1ef012f231828 (use SendMessage with to: 'a44b1ef012f231828', summary: '<5-10 word recap>' to continue this agent)
<usage>subagent_tokens: 242500
tool_uses: 132
duration_ms: 1020666</usage>
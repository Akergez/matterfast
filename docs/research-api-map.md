I have everything. Here is the implementation map.

---

# Mattermost Server API Map for a Rust Client

**Source of truth:** `mattermost/mattermost` @ `752e5d1755b5f5cc13e024c87eba3ed3622998af` (2026‑08‑21), `model.CurrentVersion` = **11.11.0**. All JSON tags below are copied verbatim from `/tmp/mm/mattermost/server/public/model/*.go`.

Key files:
- `/tmp/mm/mattermost/server/public/model/{user,post,post_metadata,post_embed,post_acknowledgement,channel,channel_member,channel_stats,channel_view,channel_sidebar,team,team_member,file_info,preference,status,custom_status,reaction,emoji,websocket_message,websocket_request,websocket_client,client4,utils}.go`
- `/tmp/mm/mattermost/server/channels/api4/{websocket,user,post,channel,team,file,status,preference,emoji,api}.go`
- `/tmp/mm/mattermost/server/channels/app/platform/{web_conn,websocket_router}.go`, `/tmp/mm/mattermost/server/channels/wsapi/{api,user,status,system,websocket_handler}.go`
- `/tmp/mm/mattermost/server/channels/web/{handlers,params,context}.go`
- `/tmp/mm/mattermost/webapp/platform/client/src/{client4.ts,websocket.ts}`

---

## 1. Auth

### Base URL
`{site_url}/api/v4` — `model.APIURLSuffix = "/api/v4"` (`client4.go:57`).

### `POST /api/v4/users/login`

Body is decoded with `model.MapFromJSON` → **`map[string]string`, so every value must be a JSON string** (`api4/user.go:2186`). Recognized keys:

```json
{
  "login_id": "user@example.com",   // email or username
  "id": "",                          // alternative: login by user id
  "password": "…",
  "token": "123456",                 // MFA code (note: NOT "mfa_token")
  "device_id": "",
  "voip_device_id": "",
  "ldap_only": "true",               // string "true", not bool
  "magic_link_token": ""
}
```

Note the JS client sends `deviceId` (camelCase) in `login()` and `device_id` in `loginById()` — only `device_id` is actually read server-side.

Rate limited: 5/sec, burst 10 (`api4/user.go:69`).

**Response:** the `User` JSON body, plus:
- Header **`Token: <session token>`** — always set by `DoLogin` (`app/login.go:221`).
- Cookies `MMAUTHTOKEN` (HttpOnly), `MMUSERID`, `MMCSRF` — **only set if the request carried `X-Requested-With: XMLHttpRequest`** (`api4/user.go:2277-2279` → `AttachSessionCookies`, `app/login.go:288`).

For a Rust client: send `X-Requested-With: XMLHttpRequest`, then read the `Token` header and use `Authorization: Bearer <token>` from then on; **do not** keep a cookie jar (see gotcha below).

### MFA
- If the account has MFA on and `token` is missing/wrong, login fails with `mfa.validate_token.authenticate.app_error` / `api.user.check_user_mfa.bad_code.app_error` (these are in the `unmaskedErrors` allow-list at `api4/user.go:2132`, so you get the real error id; all other failures are masked into `api.user.login.invalid_credentials_*`).
- Probe before login (deprecated but present): `POST /api/v4/users/mfa` `{"login_id":"…"}` → `{"mfa_required": bool}`.
- Enforcement on *other* endpoints: handlers with `RequireMfa: true` return `api.context.mfa_required.app_error` when the server enforces MFA and the session hasn't satisfied it.
- Setup: `POST /api/v4/users/{user_id}/mfa/generate`, `PUT /api/v4/users/{user_id}/mfa`.

### Personal access tokens
- `POST /api/v4/users/{user_id}/tokens` body `{"description":"…"}` → `UserAccessToken` including the plaintext `token` (only time it's returned).
- `GET /api/v4/users/{user_id}/tokens?page&per_page`, `GET /api/v4/users/tokens/{token_id}`
- `POST /api/v4/users/tokens/revoke` `{"token_id":"…"}`, `/tokens/disable`, `/tokens/enable`, `/tokens/rotate`, `/tokens/search`
- Requires `create_user_access_token` permission and `ServiceSettings.EnableUserAccessTokens`.
- A PAT is used exactly like a session token: `Authorization: Bearer <pat>`.

### Token parsing precedence — **important**
`app.ParseAuthTokenFromRequest` (`app/authentication.go:493`) checks in this order:

1. **Cookie `MMAUTHTOKEN`** → `TokenLocationCookie`
2. `Authorization: Bearer <token>` (case-insensitive on the first 6 chars) → `TokenLocationHeader`
3. `Authorization: Token <token>` (OAuth) → `TokenLocationHeader`
4. `?access_token=` query param → `TokenLocationQueryString`
5. `X-Cloud-Token`, `X-RemoteCluster-Token` headers

**The cookie wins over the header.** If reqwest is configured with `cookie_store(true)` and you also set a Bearer header, the cookie is what authenticates, which silently switches you into the CSRF-enforced path. Recommendation: `.cookie_store(false)` and header auth only.

### CSRF — exactly when required
`web.Handler.checkCSRFToken` (`web/handlers.go:508`):

```go
csrfCheckNeeded := session != nil && c.Err == nil &&
    tokenLocation == app.TokenLocationCookie &&
    !h.TrustRequester &&
    r.Method != "GET"
```

So CSRF is required **iff all four hold**: authenticated by **cookie**, non-GET method, and the route is not registered with `TrustRequester`. Bearer-header auth ⇒ **never** any CSRF check.

When needed, pass `X-CSRF-Token: <MMCSRF cookie value>` (`model.HeaderCsrfToken = "X-CSRF-Token"`). Legacy fallback: if the header is absent but `X-Requested-With: XMLHttpRequest` is present, the check passes **unless** `ServiceSettings.ExperimentalStrictCSRFEnforcement` is true (`web/handlers.go:517-533`). Failure → HTTP 401 `api.context.session_expired.app_error`.

`TrustRequester` routes (CSRF-exempt) include: `GET /api/v4/websocket`, `GET /files/{id}`, `/files/{id}/thumbnail`, `/files/{id}/preview`, `/users/{id}/image`, `/users/{id}/image/default`, `/teams/{id}/image`, `/emoji/{id}/image`, `/users/login/cws`.

### `X-Requested-With: XMLHttpRequest`
The JS client sets it on **every** request (`client4.ts:597`). Server uses it for exactly two things: (a) whether `POST /users/login` attaches session cookies, (b) the legacy CSRF fallback. Harmless and recommended to always send.

### Logout
`POST /api/v4/users/logout` — revokes the session and clears the `MMAUTHTOKEN` cookie (`MaxAge: -1`). Returns `{"status":"OK"}`. Also invalidates PATs? No — only the session identified by the current token.

### `/api/v4/users/me`
`GET /api/v4/users/me` — `"me"` is a literal alias for the session's user id (`model.Me = "me"`) and is accepted anywhere `{user_id}` appears. Supports ETag (`user.Etag(showFullName, showEmail)`).

---

## 2. Endpoints by domain

Two pagination styles coexist:
- **Offset**: `?page=0&per_page=60` — default 60, **max 200** (`web/params.go: PerPageDefault=60, PerPageMaximum=200`; values above the max are silently clamped, not rejected).
- **Cursor/time**: `?since=<ms>` / `?before=<post_id>` / `?after=<post_id>`, and `?limit_before` / `?limit_after` (default 60, max 200, `LimitDefault`/`LimitMaximum`).

### Users
| Method | Path | Params |
|---|---|---|
| GET | `/users/me` | ETag |
| GET | `/users/{user_id}` | ETag |
| GET | `/users` | `page`, `per_page`, `in_team`, `not_in_team`, `in_channel`, `not_in_channel`, `in_group`, `not_in_group`, `without_team`, `group_constrained`, `active`, `inactive`, `role`, `sort` (`""`\|`status`\|`create_at`\|`last_activity_at`\|`admin`), `cursor_id`, `abac_match_only` |
| POST | `/users/ids` | body `["id1","id2"]`; query `?since=<ms>` (returns only users updated since) |
| POST | `/users/usernames` | body `["alice","bob"]` |
| GET | `/users/username/{username}` | |
| GET | `/users/email/{email}` | |
| GET | `/users/autocomplete` | `name`, `in_team`, `in_channel`, `limit` (default 25) → `{users:[…], out_of_channel:[…]}` |
| POST | `/users/search` | body `{term, team_id, not_in_team_id, in_channel_id, not_in_channel_id, in_group_id, group_constrained, allow_inactive, without_team, limit}` |
| POST | `/users/group_channels` | body `[channel_id…]` → `{channel_id: [User…]}` |
| GET | `/users/{user_id}/status` | |
| POST | `/users/status/ids` | body `["id1",…]` → `[Status…]` |
| PUT | `/users/{user_id}/status` | body `Status` |
| PUT | `/users/me/status/custom` | body `CustomStatus` → `{"status":"OK"}` |
| DELETE | `/users/me/status/custom` | |
| DELETE/POST | `/users/me/status/custom/recent` / `…/recent/delete` | POST variant preferred (DELETE-with-body is unreliable) |
| POST | `/users/{user_id}/typing` | body `{"channel_id":"…","parent_id":"…"}` (REST twin of the WS `user_typing` action) |
| GET | `/users/me/channel_members` | `page`, `per_page` — all channel memberships across teams |
| PUT | `/users/me/patch`, `PUT /users/{id}` | |

### Teams
| Method | Path | Params |
|---|---|---|
| GET | `/users/me/teams` | teams I'm on |
| GET | `/users/me/teams/members` | `[TeamMember…]` |
| GET | `/users/me/teams/unread` | `exclude_team=<id>`, `include_collapsed_threads=true` → `[TeamUnread…]` |
| GET | `/users/{user_id}/teams/{team_id}/unread` | single `TeamUnread` |
| GET | `/teams` | `page`, `per_page`, `include_total_count`, `exclude_policy_constrained`, `for_directory` |
| GET | `/teams/{team_id}` , `/teams/name/{name}` | |
| GET | `/teams/{team_id}/members` | `page`, `per_page`, `exclude_deleted_users` |
| POST | `/teams/{team_id}/members/ids` | body `[user_id…]` |
| GET | `/users/{user_id}/teams/members` | |
| GET | `/teams/{team_id}/members/{user_id}` | |
| GET | `/teams/{team_id}/stats` | |

### Channels
| Method | Path | Params |
|---|---|---|
| GET | `/users/me/teams/{team_id}/channels` | `include_deleted`, `last_delete_at=<ms>` — **the "my channels for team" call** |
| GET | `/users/me/channels` | all teams |
| GET | `/users/me/teams/{team_id}/channels/members` | `[ChannelMember…]` for the team |
| GET | `/channels/{channel_id}` | `as_content_reviewer`, `flagged_post_id` |
| GET | `/teams/{team_id}/channels/name/{channel_name}` | `include_deleted` |
| GET | `/teams/name/{team_name}/channels/name/{channel_name}` | `include_deleted` |
| GET | `/teams/{team_id}/channels` | `page`, `per_page` (public); ETag via `ChannelList.Etag()` |
| GET | `/teams/{team_id}/channels/deleted`, `/private`, `/recommended` | `page`, `per_page` |
| GET | `/channels/{channel_id}/members` | `page`, `per_page` |
| POST | `/channels/{channel_id}/members/ids` | body `[user_id…]` |
| GET | `/channels/{channel_id}/members/{user_id}` (or `me`) | |
| PUT | `/channels/{channel_id}/members/{user_id}/notify_props` | |
| POST | `/channels/stats/member_count` | body `[channel_id…]` → `{channel_id: count}` |
| GET | `/channels/{channel_id}/stats` | `exclude_files_count=true` → `ChannelStats` |
| GET | `/users/{user_id}/channels/{channel_id}/unread` | → `ChannelUnread` |
| POST | `/channels/members/me/view` | body `ChannelView` → `ChannelViewResponse` |
| POST | `/channels/members/me/mark_read` | body `[channel_id…]` |
| PUT | `/users/{user_id}/teams/{team_id}/read` | mark whole team read |
| GET | `/teams/{team_id}/channels/autocomplete` | `name` |
| GET | `/teams/{team_id}/channels/search_autocomplete` | `name` |
| POST | `/teams/{team_id}/channels/search` | body `{term}` |

**Sidebar categories** (base = `/users/{user_id}/teams/{team_id}/channels/categories`):
`GET ""` → `OrderedSidebarCategories`; `POST ""` create; `PUT ""` bulk update; `GET /order`; `PUT /order` (body `[category_id…]`); `GET /{category_id}`; `PUT /{category_id}`; `DELETE /{category_id}`. Category id regex is `[A-Za-z0-9_-]+` because default categories use the synthetic form `{favorites|channels|direct_messages}_{userID}_{teamID}`.

### Posts
`GET /api/v4/channels/{channel_id}/posts` — the channel feed. Query params (`api4/post.go:295-352`):

| Param | Notes |
|---|---|
| `page`, `per_page` | default 0/60, max 200 |
| `since` | ms epoch. **Mutually exclusive with `before`/`after`/`page`/`per_page`.** Returns all posts *modified* since that time (so edits and deletes reappear), ordered by `create_at`, capped at 1000, and **not guaranteed consecutive** — the client must fill holes itself. |
| `before` / `after` | a post id; combine with `page`/`per_page` |
| `include_deleted` | requires system-admin |
| `skipFetchThreads`, `collapsedThreads`, `collapsedThreadsExtended` | camelCase, ParseBool |
| `type` | filter by post type |

ETag is served for the `page`, `before`, `after` branches (not for `since`).

Other post endpoints:
- `GET /users/{user_id}/channels/{channel_id}/posts/unread` — `limit_before`, `limit_after` (default 60, max 200). `limit_after=0` is rejected.
- `GET /posts/{post_id}` — `include_deleted`, `retain_content`; ETag `Etag(id, update_at)`
- `GET /posts/{post_id}/thread` — `skipFetchThreads`, `collapsedThreads`, `collapsedThreadsExtended`, `direction` (`up`/`down`), `perPage`, `fromPost`, `fromCreateAt`; ETag on `PostList.Etag()`
- `POST /posts` (body = `Post`; set `channel_id`, `message`, optional `root_id`, `file_ids`, `props`, `metadata.priority`), `PUT /posts/{id}`, `PUT /posts/{id}/patch` (`PostPatch`), `DELETE /posts/{id}`
- `POST /posts/ephemeral` body `{user_id, post}`
- `POST /posts/ids` body `[post_id…]`
- `GET /posts/{id}/edit_history`, `GET /posts/{id}/info`
- `POST /posts/{id}/pin`, `POST /posts/{id}/unpin`, `GET /channels/{id}/pinned` (ETag)
- Reactions: `POST /reactions` body `{user_id, post_id, emoji_name}`; `DELETE /users/{user_id}/posts/{post_id}/reactions/{emoji_name}`; `GET /posts/{post_id}/reactions`
- Search: `POST /teams/{team_id}/posts/search` or `POST /posts/search` (all teams). Body: `{terms, is_or_search, time_zone_offset, include_deleted_channels, page, per_page}` → `PostSearchResults` = `PostList` + `"matches": {post_id: ["term",…]}`
- Files on a post: `GET /posts/{post_id}/files/info` (ETag via `GetEtagForFileInfos`)
- `POST /users/{user_id}/posts/{post_id}/set_unread` body `{"collapsed_threads_supported":true}` → `ChannelUnread`
- `POST /users/{user_id}/posts/{post_id}/reminder` body `{"target_time": <ms>}`
- `GET /users/{user_id}/posts/flagged` — `channel_id`, `team_id`, `page`, `per_page`

### Files
- `POST /api/v4/files` — two modes:
  1. **multipart/form-data** with repeated `files` parts, plus form fields `channel_id` and (optionally) repeated `client_ids` — one per file, echoed back so you can correlate.
  2. Raw body with `?channel_id=…&filename=…` (single file).
  Extra query param `?bookmark=true` when uploading a channel-bookmark file (server ≥ 9.4).
  → `201` `{"file_infos": [FileInfo…], "client_ids": ["…"]}`
- `GET /files/{file_id}` (`?download=true` forces `Content-Disposition: attachment`), also `HEAD`
- `GET /files/{file_id}/thumbnail`, `GET /files/{file_id}/preview` (both `?download=`, both GET+HEAD)
- `GET /files/{file_id}/info` → `FileInfo`
- `GET /files/{file_id}/link` → `{"link": "https://…/files/{id}/public?h=…"}` (requires `EnablePublicLink`)
- `GET /files/{file_id}/public?h=…` — unauthenticated public route
- `POST /files/search` or `POST /teams/{team_id}/files/search`
- Cache-busting: the JS client appends `?{timestamp}` (bare, valueless) using `FileInfo.update_at`.

### Preferences
- `GET /users/{user_id}/preferences` → `[Preference…]`
- `PUT /users/{user_id}/preferences` body `[Preference…]` (upsert)
- `POST /users/{user_id}/preferences/delete` body `[Preference…]`
- `GET /users/{user_id}/preferences/{category}`
- `GET /users/{user_id}/preferences/{category}/name/{preference_name}`

Category constants worth knowing: `direct_channel_show`, `group_channel_show`, `favorite_channel`, `flagged_post`, `display_settings`, `advanced_settings`, `sidebar_settings`, `theme`, `notifications`, `custom_status`, `tutorial_step`, `recommended_next_steps`, `system_notice`.

### Custom emoji
- `POST /emoji` (multipart: `image` file + `emoji` = JSON string of `{name, creator_id}`)
- `GET /emoji?page&per_page&sort` (`sort=name`)
- `GET /emoji/{emoji_id}`, `GET /emoji/name/{emoji_name}`
- `POST /emoji/names` body `["name1",…]`
- `POST /emoji/search` body `{term, prefix_only}`
- `GET /emoji/autocomplete?name=`
- `GET /emoji/{emoji_id}/image` (TrustRequester → no CSRF), `DELETE /emoji/{emoji_id}`

### Typing
Preferred path is the WebSocket action `user_typing` (see §4). REST equivalent: `POST /users/{user_id}/typing` with `{"channel_id","parent_id"}`. Both are disabled when the server is in "busy" mode (`APISessionRequiredDisableWhenBusy` / `NewServerBusyWebSocketError`).

---

## 3. Exact serialized JSON shapes

All `int64` timestamps are **Unix milliseconds** unless noted.

### User (`user.go:84`)
```go
Id                     string      `json:"id"`
CreateAt               int64       `json:"create_at,omitempty"`
UpdateAt               int64       `json:"update_at,omitempty"`
DeleteAt               int64       `json:"delete_at"`
Username               string      `json:"username"`
Password               string      `json:"password,omitempty"`
AuthData               *string     `json:"auth_data,omitempty"`
AuthService            string      `json:"auth_service"`
Email                  string      `json:"email"`
EmailVerified          bool        `json:"email_verified,omitempty"`
Nickname               string      `json:"nickname"`
FirstName              string      `json:"first_name"`
LastName               string      `json:"last_name"`
Position               string      `json:"position"`
Roles                  string      `json:"roles"`
AllowMarketing         bool        `json:"allow_marketing,omitempty"`
Props                  StringMap   `json:"props,omitempty"`
NotifyProps            StringMap   `json:"notify_props,omitempty"`
LastPasswordUpdate     int64       `json:"last_password_update,omitempty"`
LastPictureUpdate      int64       `json:"last_picture_update,omitempty"`
FailedAttempts         int         `json:"failed_attempts,omitempty"`
Locale                 string      `json:"locale"`
Timezone               StringMap   `json:"timezone"`
MfaActive              bool        `json:"mfa_active,omitempty"`
MfaSecret              string      `json:"mfa_secret,omitempty"`
RemoteId               *string     `json:"remote_id,omitempty"`
LastActivityAt         int64       `json:"last_activity_at,omitempty"`
IsBot                  bool        `json:"is_bot,omitempty"`
BotDescription         string      `json:"bot_description,omitempty"`
BotLastIconUpdate      int64       `json:"bot_last_icon_update,omitempty"`
TermsOfServiceId       string      `json:"terms_of_service_id,omitempty"`
TermsOfServiceCreateAt int64       `json:"terms_of_service_create_at,omitempty"`
DisableWelcomeEmail    bool        `json:"disable_welcome_email"`
LastLogin              int64       `json:"last_login,omitempty"`
MfaUsedTimestamps      StringArray `json:"mfa_used_timestamps,omitempty"`
```
`StringMap` = `map[string]string`, `StringArray` = `[]string`. Rust: `Roles` is a **space-separated string** (`"system_user system_admin"`), not an array. `Timezone` keys: `useAutomaticTimezone` (`"true"`/`"false"` as strings), `automaticTimezone`, `manualTimezone`. Custom status lives in `Props["customStatus"]` as a **JSON-encoded string** of `CustomStatus`.

### CustomStatus (`custom_status.go:30`)
```go
Emoji     string    `json:"emoji"`
Text      string    `json:"text"`
Duration  string    `json:"duration"`   // thirty_minutes|one_hour|four_hours|today|this_week|date_and_time
ExpiresAt time.Time `json:"expires_at"` // RFC3339, NOT millis
```

### Team (`team.go:26`)
```go
Id                  string  `json:"id"`
CreateAt            int64   `json:"create_at"`
UpdateAt            int64   `json:"update_at"`
DeleteAt            int64   `json:"delete_at"`
DisplayName         string  `json:"display_name"`
Name                string  `json:"name"`
Description         string  `json:"description"`
Email               string  `json:"email"`
Type                string  `json:"type"`            // "O" open | "I" invite
CompanyName         string  `json:"company_name"`
AllowedDomains      string  `json:"allowed_domains"`
InviteId            string  `json:"invite_id"`
AllowOpenInvite     bool    `json:"allow_open_invite"`
LastTeamIconUpdate  int64   `json:"last_team_icon_update,omitempty"`
SchemeId            *string `json:"scheme_id"`
GroupConstrained    *bool   `json:"group_constrained"`
PolicyID            *string `json:"policy_id"`
CloudLimitsArchived bool    `json:"cloud_limits_archived"`
PolicyEnforced      bool            `json:"policy_enforced"`
PolicyActions       map[string]bool `json:"policy_actions,omitempty"`
PolicyIsActive      bool            `json:"policy_is_active"`
Recommended         bool            `json:"recommended,omitempty"`
```

### TeamMember (`team_member.go:20`)
```go
TeamId        string `json:"team_id"`
UserId        string `json:"user_id"`
Roles         string `json:"roles"`
DeleteAt      int64  `json:"delete_at"`
SchemeGuest   bool   `json:"scheme_guest"`
SchemeUser    bool   `json:"scheme_user"`
SchemeAdmin   bool   `json:"scheme_admin"`
ExplicitRoles string `json:"explicit_roles"`
CreateAt      int64  `json:"-"`   // NOT serialized
```

### TeamUnread (`team_member.go:47`)
```go
TeamId                   string `json:"team_id"`
MsgCount                 int64  `json:"msg_count"`
MentionCount             int64  `json:"mention_count"`
MentionCountRoot         int64  `json:"mention_count_root"`
MsgCountRoot             int64  `json:"msg_count_root"`
ThreadCount              int64  `json:"thread_count"`
ThreadMentionCount       int64  `json:"thread_mention_count"`
ThreadUrgentMentionCount int64  `json:"thread_urgent_mention_count"`
```

### Channel (`channel.go:84`)
```go
Id                  string             `json:"id"`
CreateAt            int64              `json:"create_at"`
UpdateAt            int64              `json:"update_at"`
DeleteAt            int64              `json:"delete_at"`
TeamId              string             `json:"team_id"`
Type                ChannelType        `json:"type"`
DisplayName         string             `json:"display_name"`
Name                string             `json:"name"`
Header              string             `json:"header"`
Purpose             string             `json:"purpose"`
LastPostAt          int64              `json:"last_post_at"`
TotalMsgCount       int64              `json:"total_msg_count"`
ExtraUpdateAt       int64              `json:"extra_update_at"`
CreatorId           string             `json:"creator_id"`
SchemeId            *string            `json:"scheme_id"`
Props               map[string]any     `json:"props"`
GroupConstrained    *bool              `json:"group_constrained"`
AutoTranslation     bool               `json:"autotranslation"`
Shared              *bool              `json:"shared"`
TotalMsgCountRoot   int64              `json:"total_msg_count_root"`
PolicyID            *string            `json:"policy_id"`
LastRootPostAt      int64              `json:"last_root_post_at"`
BannerInfo          *ChannelBannerInfo `json:"banner_info"`
PolicyEnforced      bool               `json:"policy_enforced"`
PolicyActions       map[string]bool    `json:"policy_actions,omitempty"`
PolicyIsActive      bool               `json:"policy_is_active"`
DefaultCategoryName string             `json:"default_category_name"`
ManagedCategoryName string             `json:"managed_category_name"`
Discoverable        bool               `json:"discoverable"`
```
`ChannelType` string values: `"O"` open, `"P"` private, `"D"` direct, `"G"` group, `"S"` space, `"BO"` open board, `"BP"` private board. `ChannelBannerInfo` = `{enabled: *bool, text: *string, background_color: *string}`.

Note `type` is an open-ish enum — model it as `enum { Open, Private, Direct, Group, Space, OpenBoard, PrivateBoard, Other(String) }` so a new value doesn't break deserialization.

### ChannelMember (`channel_member.go:55`)
```go
ChannelId               string    `json:"channel_id"`
UserId                  string    `json:"user_id"`
Roles                   string    `json:"roles"`
LastViewedAt            int64     `json:"last_viewed_at"`
MsgCount                int64     `json:"msg_count"`
MentionCount            int64     `json:"mention_count"`
MentionCountRoot        int64     `json:"mention_count_root"`
UrgentMentionCount      int64     `json:"urgent_mention_count"`
MsgCountRoot            int64     `json:"msg_count_root"`
NotifyProps             StringMap `json:"notify_props"`
LastUpdateAt            int64     `json:"last_update_at"`
SchemeGuest             bool      `json:"scheme_guest"`
SchemeUser              bool      `json:"scheme_user"`
SchemeAdmin             bool      `json:"scheme_admin"`
ExplicitRoles           string    `json:"explicit_roles"`
AutoTranslationDisabled bool      `json:"autotranslation_disabled"`
```
**Gotcha:** `ChannelMember` has a custom `MarshalJSON` that re-emits `last_viewed_at` / `last_update_at` as `*int64` with `omitempty`. When you fetch *another* user's membership, `SanitizeForCurrentUser` stamps `-1` and those two fields are **omitted entirely** from the JSON. In Rust make them `Option<i64>` with `#[serde(default)]`.

`notify_props` keys: `desktop`, `desktop_sound`, `desktop_threads`, `push`, `push_threads`, `email`, `mark_unread` (`all`|`mention`), `ignore_channel_mentions` (`on`|`off`|`default`), `channel_auto_follow_threads` (`on`|`off`). Values are strings; `default`|`all`|`mention`|`none`.

### ChannelUnread (`channel_member.go:31`)
```go
TeamId             string `json:"team_id"`
ChannelId          string `json:"channel_id"`
MsgCount           int64  `json:"msg_count"`
MentionCount       int64  `json:"mention_count"`
MentionCountRoot   int64  `json:"mention_count_root"`
UrgentMentionCount int64  `json:"urgent_mention_count"`
MsgCountRoot       int64  `json:"msg_count_root"`
NotifyProps        StringMap `json:"-"`   // not serialized
```
Related: `ChannelUnreadAt` adds `user_id` and `last_viewed_at` — that's what `POST /users/{id}/posts/{id}/set_unread` returns.

### ChannelStats / ChannelView (`channel_stats.go`, `channel_view.go`)
```go
type ChannelStats struct {
    ChannelId       string `json:"channel_id"`
    MemberCount     int64  `json:"member_count"`
    GuestCount      int64  `json:"guest_count"`
    PinnedPostCount int64  `json:"pinnedpost_count"`   // note: no underscore between pinned and post
    FilesCount      int64  `json:"files_count"`
}
type ChannelView struct {
    ChannelId                 string `json:"channel_id"`
    PrevChannelId             string `json:"prev_channel_id"`
    CollapsedThreadsSupported bool   `json:"collapsed_threads_supported"`
}
type ChannelViewResponse struct {
    Status            string           `json:"status"`
    LastViewedAtTimes map[string]int64 `json:"last_viewed_at_times"`
}
```
Empty-string `channel_id`/`prev_channel_id` are legal and mean "focus lost".

### Post (`post.go:137`)
```go
Id            string          `json:"id"`
CreateAt      int64           `json:"create_at"`
UpdateAt      int64           `json:"update_at"`
EditAt        int64           `json:"edit_at"`
DeleteAt      int64           `json:"delete_at"`
IsPinned      bool            `json:"is_pinned"`
UserId        string          `json:"user_id"`
ChannelId     string          `json:"channel_id"`
RootId        string          `json:"root_id"`
OriginalId    string          `json:"original_id"`
Message       string          `json:"message"`
MessageSource string          `json:"message_source,omitempty"`
Type          string          `json:"type"`
Props         StringInterface `json:"props"`         // map[string]any
Hashtags      string          `json:"hashtags"`
Filenames     StringArray     `json:"-"`             // deprecated, not serialized
FileIds       StringArray     `json:"file_ids"`
PendingPostId string          `json:"pending_post_id"`
HasReactions  bool            `json:"has_reactions,omitempty"`
RemoteId      *string         `json:"remote_id,omitempty"`
ReplyCount    int64           `json:"reply_count"`
LastReplyAt   int64           `json:"last_reply_at"`
Participants  []*User         `json:"participants"`
IsFollowing   *bool           `json:"is_following,omitempty"`
Metadata      *PostMetadata   `json:"metadata,omitempty"`
```
- `root_id` — `""` for a root post; the root's id for a reply. There is no longer a `parent_id` on the wire.
- `type` — `""` for a normal post; system posts are `system_*` (see the full constant list in `post.go:27-70`: `system_join_channel`, `system_leave_channel`, `system_add_to_channel`, `system_remove_from_channel`, `system_header_change`, `system_displayname_change`, `system_purpose_change`, `system_channel_deleted`, `system_ephemeral`, `system_convert_channel`, …), plus `slack_attachment`, `me`, `reminder`, `burn_on_read`, `card`, and the `custom_*` prefix for plugins.
- `props` — free-form. Well-known keys: `attachments` (Slack-style message attachments), `override_username`, `override_icon_url`, `override_icon_emoji`, `from_webhook`, `from_bot`, `from_plugin`, `addedUserId`, `deleteBy`, `previewed_post`, `channel_mentions`, `disable_group_highlight`, `mentionHighlightDisabled`, `mm_blocks`, `blocks`, `cards`. Limit: 800000 runes total (`PostPropsMaxRunes`).
- `message` max: 65535 bytes (`PostMessageMaxBytesV2`).

### PostMetadata (`post_metadata.go:11`)
```go
Embeds            []*PostEmbed                `json:"embeds,omitempty"`
Emojis            []*Emoji                    `json:"emojis,omitempty"`
Files             []*FileInfo                 `json:"files,omitempty"`
RedactedFileCount int                         `json:"redacted_file_count,omitempty"`
Images            map[string]*PostImage       `json:"images,omitempty"`
Reactions         []*Reaction                 `json:"reactions,omitempty"`
Priority          *PostPriority               `json:"priority,omitempty"`
Acknowledgements  []*PostAcknowledgement      `json:"acknowledgements,omitempty"`
Translations      map[string]*PostTranslation `json:"translations,omitempty"`
ExpireAt          int64                       `json:"expire_at,omitempty"`
Recipients        []string                    `json:"recipients,omitempty"`
```
Sub-shapes:
```go
type PostEmbed struct {
    Type PostEmbedType `json:"type"`          // image|message_attachment|opengraph|link|permalink|boards
    URL  string        `json:"url,omitempty"`
    Data any           `json:"data,omitempty"` // OpenGraph object, permalink Post, …
}
type PostImage struct {
    Width      int    `json:"width"`
    Height     int    `json:"height"`
    Format     string `json:"format"`       // "png"|"gif"|"jpeg"
    FrameCount int    `json:"frame_count"`  // 0 unless animated gif
}
type PostPriority struct {
    Priority                *string `json:"priority"`                 // "" | "important" | "urgent"
    RequestedAck            *bool   `json:"requested_ack"`
    PersistentNotifications *bool   `json:"persistent_notifications"`
    PostId                  string  `json:",omitempty"`   // internal; serialized as "PostId" if set
    ChannelId               string  `json:",omitempty"`   // internal; serialized as "ChannelId" if set
}
type PostAcknowledgement struct {
    UserId         string  `json:"user_id"`
    PostId         string  `json:"post_id"`
    AcknowledgedAt int64   `json:"acknowledged_at"`
    ChannelId      string  `json:"channel_id"`
    RemoteId       *string `json:"remote_id,omitempty"`
}
type PostTranslation struct {
    Text       string          `json:"text,omitempty"`
    Object     json.RawMessage `json:"object,omitempty"`
    Type       string          `json:"type"`
    State      string          `json:"state"`
    SourceLang string          `json:"source_lang,omitempty"`
}
type Emoji struct {
    Id        string `json:"id"`
    CreateAt  int64  `json:"create_at"`
    UpdateAt  int64  `json:"update_at"`
    DeleteAt  int64  `json:"delete_at"`
    CreatorId string `json:"creator_id"`
    Name      string `json:"name"`
}
```
Only `PostPriorityUrgent = "urgent"` is a declared constant; `"important"` is a valid value used by the webapp.

`PostPriority.PostId` / `ChannelId` have no `json:"name"` — Go falls back to the **field name**, so if the server ever populates them you'll see `"PostId"` / `"ChannelId"` keys. On the read path they're empty, so `omitempty` drops them. Don't include them when writing.

### PostList (`post_list.go:12`) — the shape returned by every feed endpoint
```go
Order                     []string         `json:"order"`
Posts                     map[string]*Post `json:"posts"`
NextPostId                string           `json:"next_post_id"`
PrevPostId                string           `json:"prev_post_id"`
HasNext                   *bool            `json:"has_next,omitempty"`
FirstInaccessiblePostTime int64            `json:"first_inaccessible_post_time"`
```
`order` is newest-first; `posts` is a map keyed by id. Use `next_post_id`/`prev_post_id` as cursors for the `before`/`after` params.

### FileInfo (`file_info.go:58`)
```go
Id              string  `json:"id"`
CreatorId       string  `json:"user_id"`        // NOTE: field CreatorId → JSON key "user_id"
PostId          string  `json:"post_id,omitempty"`
ChannelId       string  `json:"channel_id"`
CreateAt        int64   `json:"create_at"`
UpdateAt        int64   `json:"update_at"`
DeleteAt        int64   `json:"delete_at"`
Path            string  `json:"-"`
ThumbnailPath   string  `json:"-"`
PreviewPath     string  `json:"-"`
Name            string  `json:"name"`
Extension       string  `json:"extension"`
Size            int64   `json:"size"`
MimeType        string  `json:"mime_type"`
Width           int     `json:"width,omitempty"`
Height          int     `json:"height,omitempty"`
HasPreviewImage bool    `json:"has_preview_image,omitempty"`
MiniPreview     *[]byte `json:"mini_preview"`   // base64 JPEG blob or null
Content         string  `json:"-"`
RemoteId        *string `json:"remote_id"`
Archived        bool    `json:"archived"`
```
`mini_preview` is `Option<String>` in Rust (Go marshals `[]byte` as base64), and it is present-but-null for non-images.

### Reaction (`reaction.go:11`)
```go
UserId    string  `json:"user_id"`
PostId    string  `json:"post_id"`
EmojiName string  `json:"emoji_name"`
CreateAt  int64   `json:"create_at"`
UpdateAt  int64   `json:"update_at"`
DeleteAt  int64   `json:"delete_at"`
RemoteId  *string `json:"remote_id"`
ChannelId string  `json:"channel_id"`
```

### Preference (`preference.go:123`)
```go
UserId   string `json:"user_id"`
Category string `json:"category"`
Name     string `json:"name"`
Value    string `json:"value"`
```
`value` is always a string, even for booleans (`"true"`) and JSON blobs.

### Status (`status.go:25`)
```go
UserId         string `json:"user_id"`
Status         string `json:"status"`                    // online|away|offline|dnd|ooo
Manual         bool   `json:"manual"`
LastActivityAt int64  `json:"last_activity_at"`
ActiveChannel  string `json:"active_channel,omitempty"`  // always blanked before send
DNDEndTime     int64  `json:"dnd_end_time"`              // ** SECONDS, not millis **
PrevStatus     string `json:"-"`
```
`dnd_end_time` is the one timestamp in the API measured in **seconds** — explicitly called out in the source comment.

### SidebarCategoryWithChannels (`channel_sidebar.go:42,56`)
Embedded struct, so it flattens:
```json
{
  "id": "…",
  "user_id": "…",
  "team_id": "…",
  "sort_order": 10,
  "sorting": "" | "manual" | "recent" | "alpha",
  "type": "channels" | "direct_messages" | "favorites" | "custom" | "managed",
  "display_name": "…",
  "muted": false,
  "collapsed": false,
  "channel_ids": ["…"]      // ← field is named Channels, JSON key is channel_ids
}
```
`GET .../categories` returns `OrderedSidebarCategories`:
```json
{ "categories": [SidebarCategoryWithChannels…], "order": ["cat_id", …] }
```

### AppError — every non-2xx body (`utils.go:232`)
```json
{
  "id": "api.user.login.invalid_credentials_email_username",
  "message": "Enter a valid email or username and/or password.",
  "detailed_error": "",
  "request_id": "…",
  "status_code": 401
}
```

---

## 4. WebSocket

### Connecting
```
GET {ws|wss}://host/api/v4/websocket
```
Registered as `api.APIHandlerTrustRequester(connectWebSocket)`, GET only, trailing slash optional (`api4/websocket.go:53`). Origin is checked against `AllowCorsFrom`.

Two auth paths — **pick exactly one**:

1. **Header/cookie auth at upgrade time.** Send `Authorization: Bearer <token>` on the upgrade request (this is what `NewReliableWebSocketClientWithDialer` does), or let the `MMAUTHTOKEN` cookie ride along. Session is resolved before the upgrade; you're connected and authenticated immediately.
2. **`authentication_challenge`.** Connect anonymously, then immediately send:
```json
{"seq": 1, "action": "authentication_challenge", "data": {"token": "<session or PAT>"}}
```
   The router (`platform/websocket_router.go:38-79`) resolves the token, registers the conn with the hub, sets the user online, and replies `{"status":"OK","seq_reply":1}`. If `data.token` is missing or the session lookup fails, **the server closes the socket silently** — no error frame. Note it is a no-op if the connection already has a session token (i.e. you can't re-auth an already-authenticated socket).

The webapp uses cookie auth and only calls `authentication_challenge` when a token was explicitly supplied (`websocket.ts:271`). For a Rust client, `Authorization: Bearer` on the upgrade request via `tokio_tungstenite::connect_async` with a custom `Request` is the simplest and avoids the "cookie beats header" trap.

Until authenticated, every action except `authentication_challenge` and `presence` gets `api.web_socket_router.not_authenticated.app_error` (401).

### Request envelope (`websocket_request.go:18`)
```json
{"seq": <int64>, "action": "<string>", "data": {…}}
```
`seq` must be **> 0** (`bad_seq.app_error`) and is client-managed, monotonically increasing from 1. Empty `action` → `no_action.app_error`. Unknown action → `bad_action.app_error` (500). Messages may also be sent as **msgpack binary frames** with the same field names (`SendBinaryMessage` in `websocket_client.go`).

Actions the server understands:

| action | data | reply data |
|---|---|---|
| `authentication_challenge` | `{token}` | none |
| `ping` | none | `{text:"pong", version:"11.11.0", server_time:<ms>, node_id:""}` |
| `user_typing` | `{channel_id, parent_id}` | none (broadcasts `typing`) |
| `user_update_active_status` | `{user_is_active: bool, manual: bool}` | none |
| `get_statuses` | none | `{user_id: "online"…}` — **offline users are omitted** |
| `get_statuses_by_ids` | `{user_ids: [...]}` | `{user_id: Status}` |
| `presence` | `{channel_id}` and/or `{team_id}` and/or `{thread_channel_id, is_thread_view}` | none |
| `posted_notify_ack` | `{post_id, user_agent, status, reason, data}` | none |
| `custom_<plugin_id>_<name>` | plugin-defined | plugin-defined |

Server injects `remote_addr` and `x_forwarded_for` into `data` before handing it to plugins.

### Reply envelope (`websocket_message.go: WebSocketResponse`)
```json
{"status": "OK", "seq_reply": 1, "data": {…}}
{"status": "FAIL", "seq_reply": 1, "error": {"id":"…","message":"…","detailed_error":"","status_code":400}}
```
`seq_reply`, `data`, `error` are all `omitempty`. `status` is `"OK"` or `"FAIL"`. `DetailedError` is wiped on the error path before sending.

**Discrimination rule:** a frame with a non-zero `seq_reply` is a reply; a frame with an `event` field is an event. The webapp checks `if (msg.seq_reply)` first (`websocket.ts:352`); the Go client tries `WebSocketEventFromJSON` and falls back on `IsValid()` (`event != ""`). In Rust, deserialize into an untagged enum keyed on the presence of `event` vs `seq_reply`.

### Event envelope (`websocket_message.go: webSocketEventJSON`, `precomputedJSONBuf`)
```json
{
  "event": "posted",
  "data": { … },
  "broadcast": {
    "omit_users": null,
    "user_id": "",
    "channel_id": "abc…",
    "team_id": "",
    "connection_id": "",
    "omit_connection_id": ""
  },
  "seq": 42
}
```
`WebsocketBroadcast` also has `contains_sanitized_data`, `contains_sensitive_data`, `required_permissions` (all `omitempty`) — these are server-side routing hints and generally absent client-side. `broadcast_hooks` / `broadcast_hook_args` are stripped before send. `omit_users` is `map[string]bool` → `Option<HashMap<String,bool>>`.

`seq` here is the **server**'s stream sequence, entirely separate from your request `seq`.

### Complete event type list (`websocket_message.go:17-131`)

Posts & threads:
`posted`, `post_edited`, `post_deleted`, `post_unread`, `post_acknowledgement_added`, `post_acknowledgement_removed`, `post_translation_updated`, `post_revealed`, `post_burned`, `burn_on_read_all_revealed`, `ephemeral_message`, `thread_updated`, `thread_follow_changed`, `thread_read_changed`, `persistent_notification_triggered`, `posted_notify_ack`

Channels:
`channel_converted`, `channel_created`, `channel_deleted`, `channel_restored`, `channel_updated`, `channel_member_updated`, `channel_scheme_updated`, `channel_viewed`*, `multiple_channels_viewed`, `direct_added`, `group_added`, `channel_bookmark_created`, `channel_bookmark_updated`, `channel_bookmark_deleted`, `channel_bookmark_sorted`, `channel_access_control_updated`, `channel_join_request_created`, `channel_join_request_updated`, `shared_channel_remote_updated`

\* `channel_viewed` is **not** in the constant list in this version — it was replaced by `multiple_channels_viewed`. Don't build your read-state logic on it.

Teams:
`added_to_team`, `leave_team`, `update_team`, `delete_team`, `restore_team`, `update_team_scheme`, `team_access_control_updated`

Users & presence:
`new_user`, `user_added`, `user_updated`, `user_role_updated`, `memberrole_updated`, `user_removed`, `status_change`, `typing`, `user_activation_status_change`, `guests_deactivated`, `presence`

Preferences / sidebar:
`preference_changed`, `preferences_changed`, `preferences_deleted`, `sidebar_category_created`, `sidebar_category_updated`, `sidebar_category_deleted`, `sidebar_category_order_updated`

Reactions & emoji: `reaction_added`, `reaction_removed`, `emoji_added`

Drafts & scheduled posts: `draft_created`, `draft_updated`, `draft_deleted`, `scheduled_post_created`, `scheduled_post_updated`, `scheduled_post_deleted`

Groups: `received_group`, `received_group_associated_to_team`, `received_group_not_associated_to_team`, `received_group_associated_to_channel`, `received_group_not_associated_to_channel`, `group_member_deleted`, `group_member_add`

Custom profile attributes / properties:
`custom_profile_attributes_field_created`, `custom_profile_attributes_field_updated`, `custom_profile_attributes_field_deleted`, `custom_profile_attributes_values_updated`, `property_field_created`, `property_field_updated`, `property_field_deleted`, `property_values_updated`

Views & boards: `view_created`, `view_updated`, `view_deleted`, `view_sorted`, `board_created`

System / admin:
`hello`, `response`, `authentication_challenge`, `plugin_statuses_changed`, `plugin_enabled`, `plugin_disabled`, `role_updated`, `license_changed`, `config_changed`, `open_dialog`, `cloud_subscription_changed`, `first_admin_visit_marketplace_status_received`, `hosted_customer_signup_progress_updated`, `job_updated`, `recap_updated`, `content_flagging_report_value_updated`, `file_download_rejected`, `file_upload_rejected`, `show_toast`

Also: `WebSocketMsgTypeResponse = "response"`, `WebSocketMsgTypeEvent = "event"` (internal queue tags, not event names on the wire).

### `data` payloads for the important events

**`posted`** (`app/notification.go:691` + `app/post.go:1129`)
```json
{
  "post": "{\"id\":\"…\",\"message\":\"hi\",…}",   // ← JSON-ENCODED STRING, not an object
  "channel_type": "O",
  "channel_display_name": "Town Square",
  "channel_name": "town-square",
  "sender_name": "alice",
  "team_id": "…",
  "set_online": true,
  "otherFile": "true",        // present only if the post has non-image files
  "image": "true",            // present only if any attached file is an image
  "mentions": "[\"user_id\"]",   // JSON-encoded string array; per-recipient via broadcast hook
  "followers": "[\"user_id\"]",  // JSON-encoded string array; CRT followers
  "should_ack": true          // present when the client should send posted_notify_ack
}
```
`post` comes from `Post.ToJSON()` which returns a **Go `string`** (`post.go:402`), so it is embedded as a JSON string literal and must be parsed a second time. In serde:
```rust
#[derive(Deserialize)]
struct Posted {
    post: String,                          // then serde_json::from_str::<Post>(&post)
    channel_type: String,
    channel_display_name: String,
    channel_name: String,
    sender_name: String,
    team_id: String,
    #[serde(default)] set_online: bool,
    #[serde(default)] mentions: Option<String>,   // also a JSON string
    #[serde(default)] followers: Option<String>,
    #[serde(default)] should_ack: Option<bool>,
}
```
The same double-encoding applies to `mentions` and `followers` (`model.ArrayToJSON`).

**`post_edited`** — `{"post": "<json string>"}` (same `publishWebsocketEventForPost` path). Note the outgoing post has permalink previews, `channel_mentions` prop, and other per-recipient metadata **stripped**; re-fetch via REST if you need them.

**`post_deleted`** — `{"post": "<json string>"}`; a second, admin-only broadcast (`contains_sensitive_data`) additionally carries `{"delete_by": "<user_id>"}` (`app/post.go:3378-3387`).

**`typing`** — `{"user_id": "…", "parent_id": "…"}`. The channel is in `broadcast.channel_id`, **not** in `data`. The typing user is in `broadcast.omit_users`, so they don't see their own event.

**`status_change`** — `{"status": "online", "user_id": "…"}`.

**`multiple_channels_viewed`** — `{"channel_times": {"<channel_id>": <last_viewed_at_ms>, …}}`. Broadcast to `broadcast.user_id` only. Emitted only when `ServiceSettings.EnableChannelViewedMessages` is on.

**`post_unread`** — `{"msg_count", "msg_count_root", "mention_count", "mention_count_root", "urgent_mention_count", "last_viewed_at", "post_id"}` (all numbers except `post_id`).

**`reaction_added` / `reaction_removed`** — `{"reaction": "{\"user_id\":…,\"post_id\":…,\"emoji_name\":…}"}` — again a **JSON-encoded string** (`app/reaction.go:210`).

**`post_acknowledgement_added` / `_removed`** — `{"acknowledgement": "<json string of PostAcknowledgement>"}`.

**`user_updated`** — `{"user": {…}}` — here the user is a **real JSON object**, not a string. Three variants are broadcast with different sanitization: admin copy (`contains_sensitive_data`), general copy (`contains_sanitized_data`), and the subject's own copy (`broadcast.user_id == subject`). `WebSocketEventFromJSON` in the Go client special-cases re-parsing `data["user"]` for this reason.

**`preferences_changed` / `preferences_deleted`** — `{"preferences": "[{\"user_id\":…,\"category\":…,\"name\":…,\"value\":…}]"}` — **JSON-encoded string** of a `Preference` array (`app/preference.go:76,117`). Singular `preference_changed` (from `/expand`, `/collapse` slash commands) carries `{"preference": "<json string of one Preference>"}`.

**`hello`** — `{"server_version": "11.11.0.<build>.<config_hash>.<true|false>", "connection_id": "<26-char id>", "server_hostname": "<os.Hostname()>"}`. `server_hostname` is **omitted** if the server can't determine it. Always the first frame of a stream, `seq: 0`.

**Other useful ones:**

| event | data |
|---|---|
| `channel_created` | `{channel_id, team_id}` |
| `channel_deleted` | `{channel_id, delete_at}` |
| `channel_restored` | `{channel_id}` |
| `channel_updated` | `{channel: "<json string>", channel_id}` |
| `channel_converted` | `{channel_id, channel_type}` |
| `channel_member_updated` | `{channelMember: "<json string>"}` (camelCase key!) |
| `direct_added` | `{creator_id, teammate_id}` |
| `group_added` | `{teammate_ids: "<json string array>"}` |
| `user_added` | `{user_id, team_id}`; channel in `broadcast.channel_id` |
| `user_removed` | `{user_id, channel_id, remover_id}` |
| `new_user` | `{user_id}` |
| `user_role_updated` | `{user_id, roles}` |
| `memberrole_updated` | `{member: "<json string>"}` |
| `added_to_team` / `leave_team` | `{team_id, user_id}` |
| `update_team` / `delete_team` / `restore_team` / `update_team_scheme` | `{team: "<json string>"}` |
| `ephemeral_message` | `{post: "<json string>"}` |
| `emoji_added` | `{emoji: "<json string>"}` |
| `sidebar_category_created` / `_deleted` | `{category_id}` |
| `sidebar_category_updated` | `{updatedCategories: "<json string>"}` (camelCase!) — **and** it's fired on *every* preference update with no payload at all (see the `TODO` at `app/preference.go:66`), so treat it as "refetch categories" |
| `sidebar_category_order_updated` | `{order: [...]}` |
| `thread_updated` | `{thread: "<json string>", previous_unread_replies, previous_unread_mentions}` |
| `thread_read_changed` | `{thread_id, channel_id, timestamp, unread_mentions, unread_replies, previous_unread_mentions, previous_unread_replies}` |
| `thread_follow_changed` | `{thread_id, state, reply_count}` |
| `draft_created`/`_updated`/`_deleted` | `{draft: "<json string>"}` |
| `scheduled_post_created`/`_updated`/`_deleted` | `{scheduledPost: "<json string>"}` (camelCase!) |
| `config_changed` | `{config: {…}}` |
| `license_changed` | `{license: {…}}` |
| `plugin_statuses_changed` | `{plugin_statuses: [...]}` |
| `open_dialog` | `{dialog: "<json string>"}` |
| `file_upload_rejected` | `{channel_id, file_name, rejection_reason}` |
| `file_download_rejected` | `{channel_id, post_id, file_id, file_name, download_type, rejection_reason}` |

**Rule of thumb for Rust:** almost every event payload that carries a *model struct* carries it as a **JSON string**, not an object. The known exceptions are `user_updated` (`user` is an object), `channel_access_control_updated` (`channel` object), `config_changed`, `license_changed`, `job_updated`, `received_group`, `role_updated`. Model `data` as `serde_json::Map<String, Value>` and write per-event extractors that try `as_str()` → `from_str()` first and fall back to `from_value()`.

---

## 5. Reliable WebSockets

### Reconnect query params
```
GET /api/v4/websocket?connection_id=<id>&sequence_number=<next_expected_seq>
                     [&posted_ack=true][&disconnect_err_code=<code>]
```
(`api4/websocket.go:18-22`, `websocket.ts:208-216`, `model/websocket_client.go:73`)

- **First connection:** the webapp still sends `connection_id=&sequence_number=0` (empty string!). Server sees an empty `connection_id` and mints a fresh one.
- **`sequence_number`** = the **next** sequence you expect, i.e. `last_received_seq + 1` — the webapp maintains `serverSequence = msg.seq + 1` after each event (`websocket.ts:412`).
- **Both must be present together.** `PopulateWebConnConfig` (`app/platform/web_conn.go:164-197`) rejects the connection outright — closing the socket — if `connection_id` is a valid id but `sequence_number` is missing or unparseable. "A client must be either non-compliant or fully compliant."
- **`posted_ack=true`** opts into the `should_ack` field on `posted` events (for notification-delivery metrics).
- **`disconnect_err_code`** is telemetry only. Server validates it's 1000–1016, or 4000 (client ping timeout) / 4001 (client sequence mismatch); invalid values are ignored.

### `hello` fields
`{server_version, connection_id, server_hostname}` — see §4. `connection_id` is a 26-char Mattermost id.

### What a resume looks like

Server keeps a per-connection **dead queue** of the last **128** events (`deadQueueSize = 128`, `web_conn.go:42`). On reconnect with a `connection_id` + `sequence_number` (`web_conn.go:526-557`):

1. **`sequence_number` is in the dead queue** → the server drains from that index forward and replays the missed events. Metric `reconnectFound`. **You get no new `hello`**, your `connection_id` is unchanged, and the stream continues from where it left off.
2. **`sequence_number` is exactly one past the last queued event** (`hasMsgLoss()` false) → nothing to replay, stream continues. Metric `reconnectLossless`. Also no `hello`.
3. **Failed resume** — the connection wasn't found in the cluster at all (timeout, server restart, different cluster node — cross-node reconnect is explicitly not supported and falls back to non-reliable), *or* the sequence is older than the 128-event window:
   - `CheckWebConn` returns nil → server mints a **new** `connection_id`, sequence resets to 0.
   - or `hasMsgLoss()` is true → server clears the dead queue, calls `SetConnectionID(model.NewId())`, sets `Sequence = 0`, and **sends a fresh `hello`** as sequence 0. Metric `reconnectNotFound`.

### Detecting a failed resume (client side)
The webapp's rule (`websocket.ts:365-390`): on receiving `hello`, if you already had a non-empty `connectionId` **and** `msg.data.connection_id` differs from it → you lost messages. Fire the "missed messages" path, reset `serverSequence = 0`, and store the new `connection_id`.

Separately (`websocket.ts:393-411`): for every event, if `msg.seq !== expectedServerSequence`, close the socket with code **4001** and reconnect — do not try to patch the gap in place.

### Recommended gap-fill strategy
This is exactly what the webapp does on `missedMessageListeners`:

1. Refetch `GET /users/me/teams`, `/users/me/teams/members`, `/users/me/teams/unread`.
2. For the active team: `GET /users/me/teams/{team_id}/channels` and `/users/me/teams/{team_id}/channels/members`, plus `.../channels/categories`.
3. For each channel you care about (at minimum the visible one): `GET /channels/{id}/posts?since=<last_known_update_at_ms>` — remembering that `since` returns *modified* posts (edits/deletes included), unbounded in count up to 1000, and **may not be consecutive**, so reconcile against your local `PostList.order` and fetch a fresh page if you detect a hole.
4. Refetch `GET /users/me/preferences` and statuses (`get_statuses_by_ids` over your known user set).
5. Reset `sequence_number` to 0 and store the new `connection_id` for the *next* reconnect.

Backoff used by the reference client: 3s min, ×`failCount²` after 7 consecutive failures, capped at 5 min, plus up to 2s of jitter (`websocket.ts:26-34, 293-305`).

---

## 6. Gotchas

**Pagination limits.** `PerPageDefault = 60`, `PerPageMaximum = 200`, `LimitDefault = 60`, `LimitMaximum = 200`, `LogsPerPageMaximum = 10000` (`web/params.go:17-25`). Over-large values are **clamped, not rejected** — you'll get 200 rows and no error, so never infer "end of list" from `len(page) < requested`.

**`since` on the post feed.** Capped at 1000 posts, ordered by `create_at` but selected by `update_at`, and explicitly documented as possibly non-consecutive. It also cannot be combined with `before`/`after`/`page`/`per_page`.

**ETag / `If-None-Match`.** Send `If-None-Match: <etag>`, get `304` with the `ETag` header echoed and an empty body (`web/context.go:230`). Etags are `CurrentVersion + "." + parts...` (`model/utils.go:732`) — **so every server upgrade invalidates every cached etag**, which is intentional. Supported on:
- `GET /users/{id}`, `/users/username/{n}`, `/users/email/{e}`, `/users?in_team=`, `?not_in_team=`, `/users/{id}/image`
- `GET /posts/{id}`, `/posts/{id}/thread`, `/posts/{id}/files/info`
- `GET /channels/{id}/posts` (the `page`, `before`, `after` branches only — **not** `since`)
- `GET /channels/{id}/pinned`, `GET /teams/{id}/channels`
- `GET /teams/{id}/image`, `GET /bots`, `GET /bots/{id}`

The reference JS client does **not** use etags at all; it's an optimization you can opt into.

**Ping/pong.** Two independent mechanisms:
- *Protocol level:* the server sends WebSocket **PING control frames every 60s** (`pingInterval = pongWaitTime*6/10`) and drops the connection if no PONG arrives within `pongWaitTime = 100s` (`web_conn.go:36-38, 444-455, 623`). tokio-tungstenite auto-responds to pings if you drive the stream, but confirm your read loop is actually polling — a stalled consumer will get dropped after 100s.
- *Application level:* the webapp sends `{"action":"ping","seq":n}` **every 30s** and, if a reply doesn't arrive before the next tick, closes with code **4000** and reconnects (`websocket.ts:346-380`). The Go `WebSocketClient` instead watches for protocol pings with a `60 + 5 = 65s` timer (`PingTimeoutBufferSeconds = 5`).
Recommended for Rust: rely on protocol pings for liveness, and optionally add the 30s app-level `ping` if you want faster half-open detection.

**Message size.** `SocketMaxMessageSizeKb = 8 * 1024` = **8192 bytes**, applied as `SetReadLimit` on the server side (`web_conn.go:444`). Outbound frames larger than that are dropped by the server's read. Server send queue is 256 messages; overflow closes the connection.

**`X-Version-Id`.** Response header on most API calls, format `{version}.{build_number}.{config_hash}.{is_enterprise_ready}` — e.g. `11.11.0.20260821.abcdef.true`. The JS client only trusts it when `Cache-Control` is absent (`client4.ts:4898`), because CDN-cached responses carry a stale one. Use it to gate feature availability. Companion header `X-Cluster-Id`.

**Deleted post semantics.** `DELETE /posts/{id}` is a **soft delete**: `delete_at` becomes non-zero and the row stays. The `post_deleted` event you receive as a normal user has been through `SanitizePostMetadataForUser` — assume the message body is not reliable. Only system admins get the `delete_by` field (separate broadcast with `contains_sensitive_data`). Deleting a root post also deletes its replies. `include_deleted=true` on reads requires the `read_deleted_posts` permission / system admin.

**Edited post semantics.** `edit_at` != 0 marks an edit; `update_at` bumps on any change including reactions/pins, so don't use `update_at` to detect edits. `original_id` is set on the *archived* pre-edit copy (visible through `GET /posts/{id}/edit_history`), not on the live post. `message_source` holds the user's raw text when Mattermost rewrote `message` for presentation (image proxy) — use `message_source` when populating an edit box, falling back to `message`.

**Millisecond timestamps.** Every `*_at` field is Unix **milliseconds** (`model.GetMillis()`), with two exceptions: `Status.dnd_end_time` is **seconds**, and `CustomStatus.expires_at` is an **RFC3339 string**. In Rust use `i64` millis and convert at the edges; `0` universally means "unset".

**Other traps worth encoding in the crate:**
- `Roles` and `ExplicitRoles` are space-separated strings, not arrays.
- `FileInfo.CreatorId` serializes as `user_id`.
- `ChannelStats.PinnedPostCount` serializes as `pinnedpost_count`.
- `ChannelMember.last_viewed_at` / `last_update_at` can be **absent** for other users.
- WS event data keys are inconsistently cased: `channelMember`, `updatedCategories`, `scheduledPost`, `firstAdminVisitMarketplaceStatus`, `otherFile` are camelCase while everything else is snake_case.
- Query params `skipFetchThreads`, `collapsedThreads`, `collapsedThreadsExtended`, `totalsOnly`, `threadsOnly` are camelCase.
- `POST /users/login` decodes into `map[string]string` — booleans must be sent as `"true"`.
- `Preference.value` is always a string.
- The cookie beats the `Authorization` header in `ParseAuthTokenFromRequest`. Disable reqwest's cookie store.
- `sidebar_category_updated` fires with an empty payload on any preference change — treat it as an invalidation signal, not a delta.
agentId: a9282396c42327a44 (use SendMessage with to: 'a9282396c42327a44', summary: '<5-10 word recap>' to continue this agent)
<usage>subagent_tokens: 236064
tool_uses: 92
duration_ms: 843220</usage>
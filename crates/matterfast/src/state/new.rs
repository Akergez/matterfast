use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

use mattermost_api::Client;
use mattermost_api::models::*;

use super::app_state::AppState;
use super::reply_ledger::ReplyLedger;

impl AppState {
    pub fn new(client: Client, me: User, config: ClientConfig, crt_enabled: bool) -> Self {
        let mut users = HashMap::new();
        users.insert(me.id.clone(), me.clone());
        AppState {
            client,
            ws: None,
            me,
            crt_enabled,
            config,
            teams: Vec::new(),
            current_team: None,
            channels: HashMap::new(),
            memberships: HashMap::new(),
            categories: OrderedSidebarCategories::default(),
            current_channel: None,
            scroll_anchors: HashMap::new(),
            users,
            statuses: HashMap::new(),
            feeds: HashMap::new(),
            threads: HashMap::new(),
            thread_inbox: Vec::new(),
            thread_inbox_total: 0,
            replies: ReplyLedger::default(),
            mentions: Vec::new(),
            calls: None,
            active_calls: HashMap::new(),
            call: None,
            drafts: HashMap::new(),
            thread_drafts: HashMap::new(),
            drafts_synced: true,
            last_seen: HashMap::new(),
            preferences: Vec::new(),
            users_fetched_at: 0,
            ringing: Vec::new(),
            team_unreads: HashMap::new(),
            bots: Vec::new(),
            custom_emoji: BTreeSet::new(),
            groups: Vec::new(),
            groups_fetched_at: 0,
            unknown_handles: RefCell::new(BTreeSet::new()),
            pending_files: Vec::new(),
            search_results: Vec::new(),
            searching: false,
            search_terms: String::new(),
            search_pages: 0,
            search_more: false,
            saved_posts: HashSet::new(),
            reaction_unread: 0,
            typing: HashMap::new(),
        }
    }
}

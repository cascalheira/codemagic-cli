use super::*;

use crate::api::ApiClient;
use gantry_core::api_v3::{self, BuildFilter, V3Client};

impl App {
    /// Kicks off the startup fetches: teams first, because whether the v3
    /// listing is available decides how (and with which filters) builds are
    /// listed. The build fetch follows once teams resolve, either way.
    pub fn start_session(&mut self) {
        if self.v3_client.is_some() {
            self.fetch_teams();
        } else {
            self.fetch_builds();
        }
    }

    fn fetch_teams(&mut self) {
        let Some(client) = self.v3_client.clone() else {
            return;
        };
        self.loading_state = LoadingState::Loading;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.list_teams().await;
            let _ = tx.send(AppMessage::TeamsLoaded(result)).await;
        });
    }

    pub(crate) fn submit_onboarding(&mut self) {
        if self.api_token_input.trim().is_empty() {
            self.onboarding_error = Some("Please enter your API token.".to_string());
            return;
        }
        self.onboarding_loading = true;
        self.onboarding_error = None;
        let tx = self.tx.clone();
        let token = self.api_token_input.trim().to_string();
        tokio::spawn(async move {
            let client = ApiClient::new(token);
            let result = client.validate_token().await;
            let _ = tx.send(AppMessage::TokenValidated(result)).await;
        });
    }

    /// Fetches the first page of builds through whichever API is available.
    pub fn fetch_builds(&mut self) {
        if self.v3_listing_active() {
            self.fetch_builds_v3(true);
        } else {
            self.fetch_builds_v1();
        }
    }

    fn fetch_builds_v1(&mut self) {
        let Some(client) = self.api_client.clone() else {
            return;
        };
        self.loading_state = LoadingState::Loading;
        let tx = self.tx.clone();
        let skip = self.skip;
        let wf = self.workflow_filter.as_ref().map(|w| w.id.clone());
        tokio::spawn(async move {
            let result = client.get_builds(skip, wf.as_deref(), None).await;
            let _ = tx.send(AppMessage::BuildsLoaded(result)).await;
        });
    }

    /// The filters currently applied to the v3 listing.
    ///
    /// A workflow filter also pins the app: workflow IDs are scoped to an app,
    /// and v3 rejects `workflow_id` unless `app_id` comes with it.
    fn build_filter(&self) -> BuildFilter {
        BuildFilter {
            app_id: self.workflow_filter.as_ref().map(|w| w.app_id.clone()),
            workflow_id: self.workflow_filter.as_ref().map(|w| w.id.clone()),
            status: self.status_filter,
            ..Default::default()
        }
    }

    /// Fetches a page of builds from every team and merges them.
    ///
    /// v3 lists builds per team, so a multi-team account needs one request per
    /// team and its own cursor for each. `first_page` starts over from the
    /// newest build; otherwise each team continues from its stored cursor.
    fn fetch_builds_v3(&mut self, first_page: bool) {
        let Some(client) = self.v3_client.clone() else {
            return;
        };
        self.loading_state = LoadingState::Loading;

        let targets: Vec<(String, Option<String>)> = if first_page {
            self.teams.iter().map(|t| (t.id.clone(), None)).collect()
        } else {
            self.teams
                .iter()
                .filter_map(|t| {
                    self.cursors
                        .get(&t.id)
                        .map(|c| (t.id.clone(), Some(c.clone())))
                })
                .collect()
        };

        let filter = self.build_filter();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = fetch_all_teams(&client, targets, &filter, first_page).await;
            let _ = tx.send(AppMessage::V3BuildsLoaded(result)).await;
        });
    }

    pub fn load_more(&mut self) {
        if !self.has_more || matches!(self.loading_state, LoadingState::Loading) {
            return;
        }
        if self.v3_listing_active() {
            self.fetch_builds_v3(false);
        } else {
            self.skip = self.builds.len();
            self.fetch_builds_v1();
        }
    }

    pub fn refresh(&mut self) {
        if matches!(self.loading_state, LoadingState::Loading) {
            return;
        }
        self.skip = 0;
        self.cursors.clear();
        self.builds.clear();
        self.selected_index = 0;
        self.has_more = true;
        self.fetch_builds();
    }

    /// Silently re-fetches the first page from the API without clearing the
    /// currently displayed list or resetting the selection. Used by the
    /// background auto-refresh timer so the UI stays stable between ticks.
    pub fn soft_refresh(&mut self) {
        if matches!(self.loading_state, LoadingState::Loading) {
            return;
        }
        self.skip = 0;
        self.is_soft_refresh = true;
        self.fetch_builds();
    }

    /// Opens the selected build's Codemagic web page in the system browser.
    pub(crate) fn open_selected_build_in_browser(&self) {
        let Some(build) = self.builds.get(self.selected_index) else {
            return;
        };
        let url = gantry_core::web::build_url(&build.app_id, &build.id);
        gantry_core::web::open_in_browser(&url);
    }

    pub(crate) fn open_filter_popup(&mut self) {
        self.show_filter_popup = true;
        self.filter_column = FilterColumn::Workflow;
        self.filter_selected_index = match &self.workflow_filter {
            None => 0,
            Some(wf) => self
                .available_workflows
                .iter()
                .position(|w| w == wf)
                .map(|i| i + 1)
                .unwrap_or(0),
        };
        self.filter_status_index = match self.status_filter {
            None => 0,
            Some(s) => api_v3::BuildStatusFilter::ALL
                .iter()
                .position(|c| *c == s)
                .map(|i| i + 1)
                .unwrap_or(0),
        };
    }

    pub(crate) fn confirm_filter(&mut self) {
        let new_workflow = if self.filter_selected_index == 0 {
            None
        } else {
            self.available_workflows
                .get(self.filter_selected_index - 1)
                .cloned()
        };
        // The status column is only meaningful on the v3 listing; v1 has no
        // status parameter, so leave it unset rather than silently ignoring it.
        let new_status = if self.v3_listing_active() && self.filter_status_index > 0 {
            api_v3::BuildStatusFilter::ALL
                .get(self.filter_status_index - 1)
                .copied()
        } else {
            None
        };

        self.show_filter_popup = false;
        if new_workflow != self.workflow_filter || new_status != self.status_filter {
            self.workflow_filter = new_workflow;
            self.status_filter = new_status;
            self.skip = 0;
            self.cursors.clear();
            self.builds.clear();
            self.selected_index = 0;
            self.has_more = true;
            self.fetch_builds();
        }
    }

    /// Rows in the focused filter column, used to clamp cursor movement.
    fn filter_column_len(&self) -> usize {
        match self.filter_column {
            FilterColumn::Workflow => self.available_workflows.len() + 1,
            FilterColumn::Status => api_v3::BuildStatusFilter::ALL.len() + 1,
        }
    }

    pub(crate) fn toggle_filter_column(&mut self) {
        // Without v3 there is no status filter to move to.
        if !self.v3_listing_active() {
            return;
        }
        self.filter_column = match self.filter_column {
            FilterColumn::Workflow => FilterColumn::Status,
            FilterColumn::Status => FilterColumn::Workflow,
        };
    }

    pub(crate) fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub(crate) fn move_selection_down(&mut self) {
        if !self.builds.is_empty() && self.selected_index + 1 < self.builds.len() {
            self.selected_index += 1;
        }
    }

    /// Folds a freshly fetched page into the visible list.
    ///
    /// `replace` swaps the list wholesale (a manual refresh or a filter change);
    /// otherwise incoming builds update their existing row or are appended, so
    /// a background refresh never drops pages the user already loaded.
    ///
    /// The result is re-sorted because each team is paged independently: two
    /// teams' pages interleave by time rather than concatenating. Selection
    /// follows the build the user was on rather than the row index.
    pub(crate) fn merge_builds(&mut self, incoming: Vec<Build>, replace: bool) {
        let selected_id = self.builds.get(self.selected_index).map(|b| b.id.clone());

        if replace {
            self.builds = incoming;
        } else {
            for build in incoming {
                match self.builds.iter_mut().find(|b| b.id == build.id) {
                    Some(existing) => *existing = build,
                    None => self.builds.push(build),
                }
            }
        }

        self.builds
            .sort_by_key(|b| std::cmp::Reverse(b.display_time()));

        self.selected_index = selected_id
            .and_then(|id| self.builds.iter().position(|b| b.id == id))
            .unwrap_or(0)
            .min(self.builds.len().saturating_sub(1));
    }

    pub(crate) fn move_filter_up(&mut self) {
        let index = match self.filter_column {
            FilterColumn::Workflow => &mut self.filter_selected_index,
            FilterColumn::Status => &mut self.filter_status_index,
        };
        *index = index.saturating_sub(1);
    }

    pub(crate) fn move_filter_down(&mut self) {
        let max = self.filter_column_len().saturating_sub(1);
        let index = match self.filter_column {
            FilterColumn::Workflow => &mut self.filter_selected_index,
            FilterColumn::Status => &mut self.filter_status_index,
        };
        if *index < max {
            *index += 1;
        }
    }
}

/// Fetches one page from each target team and merges the results.
///
/// A team that fails is skipped rather than failing the whole listing — with
/// several teams, one inaccessible team should not blank the list. The error is
/// only propagated when it left nothing to show.
async fn fetch_all_teams(
    client: &V3Client,
    targets: Vec<(String, Option<String>)>,
    filter: &BuildFilter,
    first_page: bool,
) -> Result<V3Page> {
    let mut builds = Vec::new();
    let mut cursors = HashMap::new();
    let mut last_error = None;

    for (team_id, cursor) in targets {
        match client
            .list_team_builds(&team_id, filter, cursor.as_deref(), api_v3::PAGE_SIZE)
            .await
        {
            Ok(page) => {
                if let Some(next) = page.cursor {
                    cursors.insert(team_id, next);
                }
                builds.extend(page.builds);
            }
            Err(e) => last_error = Some(e),
        }
    }

    match last_error {
        Some(e) if builds.is_empty() => Err(e),
        _ => Ok(V3Page {
            builds,
            cursors,
            first_page,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let (tx, _rx) = mpsc::channel(8);
        App::new(tx, None)
    }

    /// `created_at` drives the sort, so tests date builds explicitly.
    fn build(id: &str, created_at: &str, status: &str) -> Build {
        serde_json::from_value(serde_json::json!({
            "_id": id, "appId": "a1", "status": status, "createdAt": created_at,
        }))
        .expect("valid build")
    }

    fn ids(app: &App) -> Vec<&str> {
        app.builds.iter().map(|b| b.id.as_str()).collect()
    }

    #[test]
    fn a_replacing_page_swaps_the_whole_list() {
        let mut app = test_app();
        app.builds = vec![build("old", "2026-01-01T00:00:00Z", "finished")];

        app.merge_builds(vec![build("new", "2026-02-01T00:00:00Z", "finished")], true);

        assert_eq!(ids(&app), ["new"]);
    }

    #[test]
    fn a_merging_page_updates_known_builds_in_place() {
        let mut app = test_app();
        app.builds = vec![build("b1", "2026-01-01T00:00:00Z", "building")];

        app.merge_builds(vec![build("b1", "2026-01-01T00:00:00Z", "finished")], false);

        assert_eq!(ids(&app), ["b1"]);
        assert_eq!(app.builds[0].status, "finished");
    }

    #[test]
    fn a_merging_page_appends_builds_it_has_not_seen() {
        let mut app = test_app();
        app.builds = vec![build("b1", "2026-01-02T00:00:00Z", "finished")];

        app.merge_builds(vec![build("b2", "2026-01-01T00:00:00Z", "finished")], false);

        assert_eq!(ids(&app), ["b1", "b2"]);
    }

    #[test]
    fn merged_pages_are_ordered_newest_first() {
        // Two teams page independently, so their builds interleave by time
        // rather than arriving in a single sorted run.
        let mut app = test_app();
        app.builds = vec![build("mid", "2026-01-02T00:00:00Z", "finished")];

        app.merge_builds(
            vec![
                build("oldest", "2026-01-01T00:00:00Z", "finished"),
                build("newest", "2026-01-03T00:00:00Z", "finished"),
            ],
            false,
        );

        assert_eq!(ids(&app), ["newest", "mid", "oldest"]);
    }

    #[test]
    fn the_selection_follows_its_build_when_newer_ones_arrive() {
        let mut app = test_app();
        app.builds = vec![
            build("b2", "2026-01-02T00:00:00Z", "finished"),
            build("b1", "2026-01-01T00:00:00Z", "finished"),
        ];
        app.selected_index = 1; // on b1

        app.merge_builds(vec![build("b3", "2026-01-03T00:00:00Z", "finished")], false);

        assert_eq!(ids(&app), ["b3", "b2", "b1"]);
        assert_eq!(app.selected_index, 2, "selection should still be on b1");
    }

    #[test]
    fn the_selection_stays_in_bounds_when_the_list_shrinks() {
        let mut app = test_app();
        app.builds = vec![
            build("b1", "2026-01-02T00:00:00Z", "finished"),
            build("b2", "2026-01-01T00:00:00Z", "finished"),
        ];
        app.selected_index = 1;

        // A filter change replaces the list with a shorter one.
        app.merge_builds(vec![build("b9", "2026-01-05T00:00:00Z", "failed")], true);

        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn merging_into_an_empty_list_leaves_a_usable_selection() {
        let mut app = test_app();

        app.merge_builds(Vec::new(), true);

        assert!(app.builds.is_empty());
        assert_eq!(app.selected_index, 0);
    }
}

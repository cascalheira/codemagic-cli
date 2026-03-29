use anyhow::{Context, Result, anyhow, bail};
use chrono;
use reqwest::Client;
use serde_json;

use crate::models::BuildsResponse;

const API_BASE_URL: &str = "https://api.codemagic.io";

/// Number of builds fetched per page.
pub const PAGE_SIZE: usize = 20;

/// A thin async wrapper around the Codemagic REST API.
///
/// `Client` is cheaply cloneable (it's an `Arc` internally), so `ApiClient`
/// can be cloned and moved into async tasks without overhead.
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    pub api_token: String,
}

impl ApiClient {
    pub fn new(api_token: String) -> Self {
        Self {
            client: Client::new(),
            api_token,
        }
    }

    /// Validates the API token by issuing a test request.
    ///
    /// Returns `Ok(true)` on HTTP 2xx, `Ok(false)` on 4xx/5xx, and `Err` on
    /// network failures.
    pub async fn validate_token(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{API_BASE_URL}/builds"))
            .header("x-auth-token", &self.api_token)
            .query(&[("skip", "0")])
            .send()
            .await
            .context("Network error while validating token")?;
        Ok(response.status().is_success())
    }

    /// Fetches a page of builds.
    ///
    /// * `skip`        – number of builds to skip (for pagination)
    /// * `workflow_id` – optional workflow filter
    /// * `app_id`      – optional application filter
    pub async fn get_builds(
        &self,
        skip: usize,
        workflow_id: Option<&str>,
        app_id: Option<&str>,
    ) -> Result<BuildsResponse> {
        let mut params: Vec<(&str, String)> = vec![("skip", skip.to_string())];
        if let Some(wid) = workflow_id {
            params.push(("workflowId", wid.to_string()));
        }
        if let Some(aid) = app_id {
            params.push(("appId", aid.to_string()));
        }

        let response = self
            .client
            .get(format!("{API_BASE_URL}/builds"))
            .header("x-auth-token", &self.api_token)
            .query(&params)
            .send()
            .await
            .context("Failed to send builds request")?;

        if !response.status().is_success() {
            bail!("API error: HTTP {}", response.status());
        }

        let text = response
            .text()
            .await
            .context("Failed to read builds response body")?;

        serde_json::from_str::<BuildsResponse>(&text).map_err(|err| {
            let snippet = &text[..text.len().min(1000)];
            anyhow!("Failed to parse builds response: {err}\n\nRaw (first 1000 chars):\n{snippet}")
        })
    }

    /// Returns full build details, including artefacts and build actions.
    pub async fn get_build(&self, build_id: &str) -> Result<crate::models::BuildDetailResponse> {
        let response = self
            .client
            .get(format!("{API_BASE_URL}/builds/{build_id}"))
            .header("x-auth-token", &self.api_token)
            .send()
            .await
            .context("Failed to fetch build details")?;

        if !response.status().is_success() {
            bail!("API error: HTTP {}", response.status());
        }

        let text = response
            .text()
            .await
            .context("Failed to read build detail body")?;
        serde_json::from_str::<crate::models::BuildDetailResponse>(&text).map_err(|err| {
            let snippet = &text[..text.len().min(800)];
            anyhow!("Failed to parse build detail: {err}\n\nRaw:\n{snippet}")
        })
    }

    /// Turns a secure artifact URL into a 1-hour public URL.
    pub async fn create_artifact_public_url(&self, artifact_url: &str) -> Result<String> {
        let expires_at = (chrono::Utc::now() + chrono::TimeDelta::hours(1)).timestamp();
        let endpoint = format!("{artifact_url}/public-url");

        let response = self
            .client
            .post(&endpoint)
            .header("x-auth-token", &self.api_token)
            .json(&serde_json::json!({ "expiresAt": expires_at }))
            .send()
            .await
            .context("Failed to create artifact public URL")?;

        if !response.status().is_success() {
            bail!("API error: HTTP {} for public URL", response.status());
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse public URL response")?;
        json["url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("No 'url' field in public URL response"))
    }

    /// Downloads raw log text from a build-action `logUrl`.
    pub async fn fetch_log(&self, log_url: &str) -> Result<String> {
        let response = self
            .client
            .get(log_url)
            .header("x-auth-token", &self.api_token)
            .send()
            .await
            .context("Failed to fetch log")?;

        if !response.status().is_success() {
            bail!("Log fetch error: HTTP {}", response.status());
        }

        response.text().await.context("Failed to read log content")
    }

    /// Returns all applications the authenticated user has access to.
    pub async fn get_apps(&self) -> Result<Vec<crate::models::Application>> {
        let response = self
            .client
            .get(format!("{API_BASE_URL}/apps"))
            .header("x-auth-token", &self.api_token)
            .send()
            .await
            .context("Failed to fetch apps")?;

        if !response.status().is_success() {
            bail!("API error: HTTP {}", response.status());
        }

        let text = response
            .text()
            .await
            .context("Failed to read apps response body")?;

        let data = serde_json::from_str::<crate::models::AppsResponse>(&text).map_err(|err| {
            let snippet = &text[..text.len().min(600)];
            anyhow!("Failed to parse apps response: {err}\n\nRaw:\n{snippet}")
        })?;

        Ok(data.applications)
    }

    /// Triggers a new build.  Returns the new build's ID on success.
    pub async fn start_build(
        &self,
        app_id: &str,
        workflow_id: &str,
        branch: &str,
    ) -> Result<String> {
        let body = serde_json::json!({
            "appId": app_id,
            "workflowId": workflow_id,
            "branch": branch,
        });

        let response = self
            .client
            .post(format!("{API_BASE_URL}/builds"))
            .header("x-auth-token", &self.api_token)
            .json(&body)
            .send()
            .await
            .context("Failed to start build")?;

        if !response.status().is_success() {
            let status = response.status();
            let msg = response.text().await.unwrap_or_default();
            bail!("API error {status}: {msg}");
        }

        let data: crate::models::StartBuildResponse = response
            .json()
            .await
            .context("Failed to parse start-build response")?;

        Ok(data.build_id)
    }

    /// Downloads any URL to a local path (streaming via byte buffer).
    pub async fn download_file(&self, url: &str, dest: &std::path::Path) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to start download")?;

        if !response.status().is_success() {
            bail!("Download error: HTTP {}", response.status());
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read download bytes")?;
        let mut file = tokio::fs::File::create(dest)
            .await
            .context("Failed to create destination file")?;
        file.write_all(&bytes)
            .await
            .context("Failed to write file")?;
        Ok(())
    }
}

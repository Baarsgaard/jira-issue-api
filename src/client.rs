use crate::models::*;
use base64::{engine::general_purpose, Engine as _};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, ClientBuilder, Response, Url};
use std::{convert::From, time::Duration};
use thiserror::Error;
use url::ParseError;

#[derive(Error, Debug)]
pub enum JiraClientError {
    #[error("Request failed")]
    HttpError(#[from] reqwest::Error),
    #[error("Authentication failed")]
    JiraQueryAuthenticationError(),
    #[error("Body malformed or invalid: {0}")]
    JiraRequestBodyError(String),
    #[error("Unable to parse response: {0}")]
    JiraResponseDeserializeError(String),
    #[error("Unable to build JiraAPIClient struct:{0}")]
    ConfigError(String),
    #[error("Unable to parse Url: {0}")]
    UrlParseError(#[from] ParseError),
    #[error("{0}")]
    TryFromError(String),
    #[error("{0}")]
    UnknownError(String),
}

/// JiraApiClient config object
#[derive(Debug, Clone)]
pub struct JiraClientConfig {
    pub credential: Credential,
    pub max_query_results: u32,
    pub url: String,
    pub timeout: u64,
    pub tls_accept_invalid_certs: bool,
}

/// Supported Authentication methods
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// Anonymous
    /// Omit Authorization header
    Anonymous,
    /// User email/username and token
    /// Authorization: Basic <b64 login:token>
    ApiToken { login: String, token: String },
    /// Personal Access Token
    /// Authorization: Bearer <PAT>
    PersonalAccessToken(String),
}

/// Reusable client for interfacing with Jira
#[derive(Debug, Clone)]
pub struct JiraAPIClient {
    pub url: Url,

    pub(crate) client: Client,
    pub(crate) anonymous_access: bool,
    pub(crate) max_results: u32,
}

impl JiraAPIClient {
    fn api_url(&self, path: &str) -> Result<Url, JiraClientError> {
        Ok(self.url.join(&format!("rest/api/latest/{}", path))?)
    }

    fn build_headers(credentials: &Credential) -> HeaderMap {
        let header_content = HeaderValue::from_static("application/json");

        let auth_header = match credentials {
            Credential::Anonymous => None,
            Credential::ApiToken {
                login: user_login,
                token: api_token,
            } => {
                let jira_encoded_auth = general_purpose::STANDARD_NO_PAD
                    .encode(format!("{}:{}", user_login, api_token,));
                Some(HeaderValue::from_str(&format!("Basic {}", jira_encoded_auth)).unwrap())
            }
            Credential::PersonalAccessToken(token) => {
                Some(HeaderValue::from_str(&format!("Bearer {}", token)).unwrap())
            }
        };

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, header_content.clone());
        headers.insert(CONTENT_TYPE, header_content);

        if let Some(mut auth_header_value) = auth_header {
            auth_header_value.set_sensitive(true);
            headers.insert(AUTHORIZATION, auth_header_value);
        }

        headers
    }

    /// Instantiate a reusable API client.
    ///
    /// ```rust
    /// use jira_issue_api::models::*;
    /// use jira_issue_api::{Credential, JiraClientConfig, JiraAPIClient};
    ///
    /// let anon = Credential::Anonymous;
    ///
    /// // let credential = Credential::PersonalAccessToken("xxxxxxx".to_string())
    ///
    /// // let api_token = Credential::ApiToken {
    /// //     login: "user@example.com".to_string(),
    /// //     token: "xxxxxxx".to_string(),
    /// // };
    ///
    /// let jira_cfg = JiraClientConfig {
    ///     credential: anon,
    ///     max_query_results: 50u32,
    ///     url: "https://domain.atlassian.net".to_string(),
    ///     timeout: 10u64,
    ///     tls_accept_invalid_certs: false,
    /// };
    ///
    /// let client = JiraAPIClient::new(&jira_cfg).unwrap();
    /// ```
    pub fn new(cfg: &JiraClientConfig) -> Result<JiraAPIClient, JiraClientError> {
        let client = ClientBuilder::new()
            .default_headers(JiraAPIClient::build_headers(&cfg.credential))
            .danger_accept_invalid_certs(cfg.tls_accept_invalid_certs)
            .https_only(true)
            .timeout(Duration::from_secs(cfg.timeout))
            .connection_verbose(false)
            .build()?;

        let mut url = Url::parse(&cfg.url)?;
        url.set_path("/");
        url.set_query(None);
        url.set_fragment(None);

        Ok(JiraAPIClient {
            url,
            client,
            max_results: cfg.max_query_results,
            anonymous_access: cfg.credential.eq(&Credential::Anonymous),
        })
    }

    pub async fn query_issues(
        &self,
        query: &str,
        fields: Option<Vec<String>>,
        expand_options: Option<Vec<String>>,
    ) -> Result<PostIssueQueryResponseBody, JiraClientError> {
        let url = self.api_url("search")?;

        let body = PostIssueQueryBody {
            jql: query.to_owned(),
            start_at: 0,
            max_results: self.max_results,
            expand: expand_options,
            fields,
        };

        let res = self.client.post(url).json(&body).send().await?;

        if !self.anonymous_access
            && (res
                .headers()
                .get("x-seraph-loginreason")
                .is_some_and(|e| e.to_str().unwrap_or_default() == "AUTHENTICATED_FAILED")
                || res
                    .headers()
                    .get("x-ausername")
                    .is_some_and(|e| e.to_str().unwrap_or_default() == "anonymous"))
        {
            return Err(JiraClientError::JiraQueryAuthenticationError());
        }

        let response = res.json::<PostIssueQueryResponseBody>().await?;
        Ok(response)
    }

    pub async fn post_worklog(
        &self,
        issue_key: &IssueKey,
        body: PostWorklogBody,
    ) -> Result<Response, JiraClientError> {
        let url = self.api_url(&format!("issue/{}/worklog", issue_key))?;

        // If any pattern matches, do not prompt.
        if matches!(
            (body.time_spent.is_some(), body.time_spent_seconds.is_some()),
            (false, false) | (true, true)
        ) {
            return Err(JiraClientError::JiraRequestBodyError(
                "time_spent and time_spent_seconds are both 'Some()' or 'None'".to_string(),
            ));
        }

        let response = self.client.post(url).json(&body).send().await?;
        Ok(response)
    }

    pub async fn post_comment(
        &self,
        issue_key: &IssueKey,
        body: PostCommentBody,
    ) -> Result<Response, JiraClientError> {
        let url = self.api_url(&format!("issue/{}/comment", issue_key))?;

        let response = self.client.post(url).json(&body).send().await?;
        Ok(response)
    }

    pub async fn get_issue(
        &self,
        issue_key: &IssueKey,
        expand_options: Option<&str>,
    ) -> Result<Issue, JiraClientError> {
        let mut url = self.api_url(&format!("issue/{}", issue_key))?;

        match expand_options {
            Some(expand_options) if !expand_options.starts_with("expand=") => {
                url.set_query(Some(&format!("expand={expand_options}")))
            }
            expand_options => url.set_query(expand_options),
        }

        let response = self.client.get(url).send().await?;
        let body = response.json::<Issue>().await?;
        Ok(body)
    }

    pub async fn get_transitions(
        &self,
        issue_key: &IssueKey,
        expand_options: Option<&str>,
    ) -> Result<GetTransitionsBody, JiraClientError> {
        let mut url = self.api_url(&format!("issue/{}/transitions", issue_key))?;

        if expand_options.is_none() {
            url.set_query(Some("expand=transitions.fields"));
        } else if expand_options.is_some() && expand_options.unwrap().starts_with("expand=") {
            url.set_query(expand_options);
        } else {
            url.set_query(Some(&format!("expand={}", expand_options.unwrap())));
        }

        let response = self.client.get(url).send().await?;
        let body = response.json::<GetTransitionsBody>().await?;
        Ok(body)
    }

    pub async fn post_transition(
        &self,
        issue_key: &IssueKey,
        transition: &PostTransitionBody,
    ) -> Result<Response, JiraClientError> {
        let url = self.api_url(&format!("issue/{}/transitions", issue_key))?;

        let response = self.client.post(url).json(transition).send().await?;
        Ok(response)
    }

    pub async fn get_assignable_users(
        &self,
        params: &GetAssignableUserParams,
    ) -> Result<Vec<User>, JiraClientError> {
        let mut url = self.api_url("user/assignable/search")?;
        let mut query: String = format!("maxResults={}", params.max_results.unwrap_or(1000));

        if params.project.is_none() && params.issue_key.is_none() {
            Err(JiraClientError::JiraRequestBodyError(
                "Both project and issue_key are None, define either to query for assignable users."
                    .to_string(),
            ))?
        }

        if let Some(issue_key) = params.issue_key.clone() {
            query.push_str(&format!("&issueKey={}", issue_key));
        }
        if let Some(username) = params.username.clone() {
            #[cfg(feature = "cloud")]
            query.push_str(&format!("&query={}", username));
            #[cfg(not(feature = "cloud"))]
            query.push_str(&format!("&username={}", username));
        }
        if let Some(project) = params.project.clone() {
            query.push_str(&format!("&project={}", project));
        }

        url.set_query(Some(&query));

        let response = self.client.get(url).send().await?;
        let body = response.json::<Vec<User>>().await?;
        Ok(body)
    }

    pub async fn post_assign_user(
        &self,
        issue_key: &IssueKey,
        user: &User,
    ) -> Result<Response, JiraClientError> {
        let url = self.api_url(&format!("issue/{}/assignee", issue_key))?;

        let body = PostAssignBody::from(user.clone());
        let response = self.client.put(url).json(&body).send().await?;
        Ok(response)
    }

    /// cloud:       user.account_id
    /// data-center: user.name
    pub async fn get_user(&self, user: &str) -> Result<User, JiraClientError> {
        let url = self.api_url("user")?;

        let key = match cfg!(feature = "cloud") {
            true => "accountId",
            false => "username",
        };

        let response = self.client.get(url).query(&[(key, user)]).send().await?;
        let body = response.json::<User>().await?;
        Ok(body)
    }

    pub async fn get_fields(&self) -> Result<Vec<Field>, JiraClientError> {
        let url = self.api_url("field")?;

        let response = self.client.get(url).send().await?;
        let body = response.json::<Vec<Field>>().await?;
        Ok(body)
    }

    pub async fn get_filter(&self, id: &str) -> Result<Filter, JiraClientError> {
        let url = self.api_url(&format!("filter/{}", id))?;

        let response = self.client.get(url).send().await?;
        let body = response.json::<Filter>().await?;
        Ok(body)
    }

    #[cfg(feature = "cloud")]
    pub async fn search_filters(
        &self,
        filter: Option<&str>,
    ) -> Result<GetFilterSearchResponseBody, JiraClientError> {
        let mut url = self.api_url("filter/search")?;
        let query = if let Some(filter) = filter {
            format!(
                "expand=jql&maxResults={}&filterName={}",
                self.max_results, filter
            )
        } else {
            format!("expand=jql&maxResults={}", self.max_results)
        };

        url.set_query(Some(&query));

        let response = self.client.get(url).send().await?;
        let body = response.json::<GetFilterSearchResponseBody>().await?;
        Ok(body)
    }
}

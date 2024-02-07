use crate::types::*;
use base64::{engine::general_purpose, Engine as _};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, ClientBuilder, Response, Url};
use std::{convert::From, time::Duration};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JiraClientError {
    #[error("Request failed")]
    HttpError(#[from] reqwest::Error),
    #[error("Body malformed or invalid: {0}")]
    JiraRequestBodyError(String),
    #[error("Unable to parse response: {0}")]
    JiraResponseDeserializeError(String),
    #[error("Unable to build JiraAPIClient struct:{0}")]
    ConfigError(String),
    #[error("Unable to parse Url: {0}")]
    UrlParseError(String),
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
#[derive(Debug, Clone)]
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
    pub version: String,

    pub(crate) client: Client,
    pub(crate) max_results: u32,
}

impl JiraAPIClient {
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
    /// use jira_issue_api::types::*;
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
            .build()
            .map_err(|e| JiraClientError::ConfigError(e.to_string()))?;

        Ok(JiraAPIClient {
            url: Url::parse(cfg.url.as_str())
                .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?,
            version: String::from("latest"),
            client,
            max_results: cfg.max_query_results,
        })
    }

    pub async fn query_issues(
        &self,
        query: &String,
    ) -> Result<PostIssueQueryResponseBody, JiraClientError> {
        let search_url = self
            .url
            .join("/rest/api/latest/search")
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;
        let body = PostIssueQueryBody {
            jql: query.to_owned(),
            start_at: 0,
            max_results: self.max_results,
            fields: vec![String::from("summary")],
        };

        let response = self
            .client
            .post(search_url)
            .json(&body)
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;

        response
            .json::<PostIssueQueryResponseBody>()
            .await
            .map_err(|e| JiraClientError::JiraResponseDeserializeError(e.to_string()))
    }

    pub async fn post_worklog(
        &self,
        issue_key: &IssueKey,
        body: PostWorklogBody,
    ) -> Result<Response, JiraClientError> {
        let worklog_url = match self
            .url
            .join(format!("/rest/api/latest/issue/{}/worklog", issue_key).as_str())
        {
            Ok(url) => url,
            Err(e) => Err(JiraClientError::UrlParseError(e.to_string()))?,
        };

        // If any pattern matches, do not prompt.
        if matches!(
            (body.time_spent.is_some(), body.time_spent_seconds.is_some()),
            (false, false) | (true, true)
        ) {
            return Err(JiraClientError::JiraRequestBodyError(
                "time_spent and time_spent_seconds are both 'Some()' or 'None'".to_string(),
            ));
        }

        self.client
            .post(worklog_url)
            .json(&body)
            .send()
            .await
            .map_err(JiraClientError::HttpError)
    }

    pub async fn post_comment(
        &self,
        issue_key: &IssueKey,
        body: PostCommentBody,
    ) -> Result<Response, JiraClientError> {
        let comment_url = self
            .url
            .join(format!("/rest/api/latest/issue/{}/comment", issue_key).as_str())
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;

        self.client
            .post(comment_url)
            .json(&body)
            .send()
            .await
            .map_err(JiraClientError::HttpError)
    }

    pub async fn get_transitions(
        &self,
        issue_key: &IssueKey,
        expand: bool,
    ) -> Result<GetTransitionsBody, JiraClientError> {
        let mut transitions_url = self
            .url
            .join(format!("/rest/api/latest/issue/{}/transitions", issue_key).as_str())
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;

        if expand {
            transitions_url.set_query(Some("expand=transitions.fields"))
        }

        let response = self
            .client
            .get(transitions_url)
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;

        response
            .json::<GetTransitionsBody>()
            .await
            .map_err(|e| JiraClientError::JiraResponseDeserializeError(e.to_string()))
    }

    pub async fn post_transition(
        &self,
        issue_key: &IssueKey,
        transition: &PostTransitionBody,
    ) -> Result<Response, JiraClientError> {
        let transition_url = self
            .url
            .join(format!("/rest/api/latest/issue/{}/transitions", issue_key).as_str())
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;

        let response = self
            .client
            .post(transition_url)
            .json(transition)
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;

        Ok(response)
    }

    pub async fn get_assignable_users(
        &self,
        params: &GetAssignableUserParams,
    ) -> Result<Vec<User>, JiraClientError> {
        let mut users_url = self
            .url
            .join("/rest/api/latest/user/assignable/search")
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;
        let mut query: String = format!("maxResults={}", params.max_results.unwrap_or(1000));

        if params.project.is_none() && params.issue_key.is_none() {
            Err(JiraClientError::JiraRequestBodyError(
                "Both project and issue_key are None, define either to query for assignable users."
                    .to_string(),
            ))?
        }

        if let Some(issue_key) = params.issue_key.clone() {
            query.push_str(format!("&issueKey={}", issue_key).as_str());
        }
        if let Some(username) = params.username.clone() {
            #[cfg(feature = "cloud")]
            query.push_str(format!("&query={}", username).as_str());
            #[cfg(not(feature = "cloud"))]
            query.push_str(format!("&username={}", username).as_str());
        }
        if let Some(project) = params.project.clone() {
            query.push_str(format!("&project={}", project).as_str());
        }

        users_url.set_query(Some(query.as_str()));

        let response = self
            .client
            .get(users_url)
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;

        response
            .json::<Vec<User>>()
            .await
            .map_err(|e| JiraClientError::JiraResponseDeserializeError(e.to_string()))
    }

    pub async fn post_assign_user(
        &self,
        issue_key: &IssueKey,
        user: &User,
    ) -> Result<Response, JiraClientError> {
        let assign_url = self
            .url
            .join(format!("/rest/api/latest/issue/{}/assignee", issue_key).as_str())
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;

        let body = PostAssignBody::from(user.clone());

        let response = self
            .client
            .put(assign_url)
            .json(&body)
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;
        Ok(response)
    }

    /// cloud:  user.account_id
    /// server: user.name
    pub async fn get_user(&self, user: String) -> Result<User, JiraClientError> {
        let user_url = self
            .url
            .join("/rest/api/latest/user")
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;

        let key = match cfg!(feature = "cloud") {
            true => "accountId",
            false => "username",
        };

        let response = self
            .client
            .get(user_url)
            .query(&[(key, user)])
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;

        response
            .json::<User>()
            .await
            .map_err(|e| JiraClientError::JiraResponseDeserializeError(e.to_string()))
    }

    #[cfg(feature = "cloud")]
    pub async fn search_filters(
        &self,
        filter: Option<String>,
    ) -> Result<GetFilterResponseBody, JiraClientError> {
        let mut search_url = self
            .url
            .join("/rest/api/latest/filter/search")
            .map_err(|e| JiraClientError::UrlParseError(e.to_string()))?;
        let query = if let Some(filter) = filter {
            format!(
                "expand=jql&maxResults={}&filterName={}",
                self.max_results, filter
            )
        } else {
            format!("expand=jql&maxResults={}", self.max_results)
        };

        search_url.set_query(Some(query.as_str()));

        let response = self
            .client
            .get(search_url)
            .send()
            .await
            .map_err(JiraClientError::HttpError)?;

        response
            .json::<GetFilterResponseBody>()
            .await
            .map_err(JiraClientError::HttpError)
    }
}

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Display, Error, Formatter},
    sync::OnceLock,
};

#[cfg(not(feature = "cloud"))]
mod versioned {
    use super::*;

    #[derive(Deserialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct User {
        pub active: bool,
        pub display_name: String,
        pub deleted: Option<bool>,
        pub name: String,
    }

    #[derive(Serialize, Debug, Clone)]
    pub struct PostAssignBody {
        pub name: String,
    }

    impl From<User> for PostAssignBody {
        fn from(value: User) -> Self {
            PostAssignBody { name: value.name }
        }
    }
}

#[cfg(feature = "cloud")]
mod versioned {
    use super::*;

    #[derive(Deserialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct GetFilterResponseBody {
        // https://developer.atlassian.com/cloud/jira/platform/rest/v2/api-group-filters/#api-rest-api-2-filter-search-get
        pub max_results: u32,
        pub start_at: u32,
        pub total: u32,
        pub is_last: bool,
        #[serde(alias = "values")]
        pub filters: Vec<Filter>,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct Filter {
        pub id: String,
        pub jql: String,
        pub name: String,
    }

    impl Filter {
        pub fn filter_query(filter: &Filter) -> String {
            format!("filter={}", filter.id)
        }
    }

    impl Display for Filter {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            write!(f, "{}: {}", self.name, self.jql)
        }
    }

    #[derive(Deserialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct User {
        pub active: bool,
        pub display_name: String,
        pub account_id: String,
        pub email_address: String,
    }

    #[derive(Serialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct PostAssignBody {
        pub account_id: String,
    }

    impl From<User> for PostAssignBody {
        fn from(value: User) -> Self {
            PostAssignBody {
                account_id: value.account_id,
            }
        }
    }
}

pub use versioned::*;

use crate::JiraClientError;

/// Define query parameters
#[derive(Debug, Clone)]
pub struct GetAssignableUserParams {
    pub username: Option<String>,
    pub project: Option<String>,
    pub issue_key: Option<IssueKey>,
    pub max_results: Option<u32>,
}

/// Comment related types
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostCommentBody {
    pub body: String,
}

/// Worklog related types
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostWorklogBody {
    pub comment: String,
    pub started: String,
    pub time_spent: Option<String>,
    pub time_spent_seconds: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
/// If duration unit is unspecififed, defaults to minutes.
pub struct WorklogDuration(String);

impl Display for WorklogDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

static WORKLOG_RE: OnceLock<Regex> = OnceLock::new();

impl TryFrom<String> for WorklogDuration {
    type Error = JiraClientError;
    fn try_from(value: String) -> Result<Self, JiraClientError> {
        let worklog_re = WORKLOG_RE.get_or_init(|| {
            Regex::new(r"([0-9]+(?:\.[0-9]+)?)[WwDdHhMm]?").expect("Unable to compile WORKLOG_RE")
        });

        let mut worklog = match worklog_re.captures(&value) {
            Some(c) => match c.get(0) {
                Some(worklog_match) => Ok(worklog_match.as_str().to_lowercase()),
                None => Err(JiraClientError::TryFromError(
                    "First capture is none: WORKLOG_RE".to_string(),
                )),
            },
            None => Err(JiraClientError::TryFromError(
                "Malformed worklog duration".to_string(),
            )),
        }?;

        let multiplier = match worklog.pop() {
            Some('m') => 60,
            Some('h') => 3600,
            Some('d') => 3600 * 8,     // 8 Hours is default for cloud.
            Some('w') => 3600 * 8 * 5, // 5 days of work in a week.
            Some(maybe_digit) if maybe_digit.is_ascii_digit() => {
                worklog.push(maybe_digit); // Unit was omitted
                60
            }
            _ => 60, // Should never reach this due to the Regex Match, but try parsing input anyways.
        };

        let seconds = worklog.parse::<f64>().map_err(|_| {
            JiraClientError::TryFromError("Unexpected worklog duration input".to_string())
        })? * f64::from(multiplier);

        Ok(WorklogDuration(format!("{:.0}", seconds)))
    }
}

/// Issue related types
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostIssueQueryBody {
    pub fields: Vec<String>,
    pub jql: String,
    pub max_results: u32,
    pub start_at: u32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostIssueQueryResponseBody {
    /// https://docs.atlassian.com/software/jira/docs/api/REST/7.6.1/#api/2/search
    pub expand: Option<String>,
    pub issues: Option<Vec<Issue>>,
    pub max_results: Option<u32>,
    pub start_at: Option<u32>,
    pub total: Option<u32>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub expand: String,
    pub fields: PostIssueQueryResponseBodyFields,
    pub id: String,
    pub key: IssueKey,
    #[serde(alias = "self")]
    pub self_reference: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostIssueQueryResponseBodyFields {
    pub summary: String,
}

impl Display for Issue {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} {}", self.key, self.fields.summary)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IssueKey(pub String);

impl Display for IssueKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

static ISSUE_RE: OnceLock<Regex> = OnceLock::new();

impl TryFrom<String> for IssueKey {
    type Error = JiraClientError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let issue_re = ISSUE_RE
            .get_or_init(|| Regex::new(r"([A-Z]{2,}-[0-9]+)").expect("Unable to compile ISSUE_RE"));

        let upper = value.to_uppercase();
        let issue_key = match issue_re.captures(&upper) {
            Some(c) => match c.get(0) {
                Some(cap) => Ok(cap),
                None => Err(JiraClientError::TryFromError(
                    "First capture is none: ISSUE_RE".to_string(),
                )),
            },
            None => Err(JiraClientError::TryFromError(
                "Malformed issue key supplied".to_string(),
            )),
        }?;

        Ok(IssueKey(issue_key.as_str().to_string()))
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetTransitionsBody {
    pub expand: String,
    pub transitions: Vec<Transition>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Transition {
    pub fields: HashMap<String, TransitionExpandedFields>,
    pub id: String,
    pub name: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransitionExpandedFields {
    pub required: bool,
    pub name: String,
    pub operations: Vec<String>,
    pub schema: TransitionExpandedFieldsSchema,
    pub allowed_values: Option<Vec<TransitionFieldAllowedValue>>,
    pub has_default_value: Option<bool>,
    pub default_value: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TransitionFieldAllowedValue {
    Str(String),
    Object {
        #[serde(alias = "self")]
        self_reference: String,
        #[serde(alias = "name")]
        value: String,
        id: String,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransitionExpandedFieldsSchema {
    #[serde(alias = "type")]
    pub schema_type: String,
    pub items: String,
    pub custom: String,
    pub custom_id: u32,
    #[cfg(not(feature = "cloud"))]
    pub system: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PostTransitionIdBody {
    pub id: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct PostTransitionFieldBody {
    pub name: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct PostTransitionBody {
    pub transition: PostTransitionIdBody,
    pub fields: Option<HashMap<String, PostTransitionFieldBody>>,
    pub update: Option<PostTransitionUpdateField>,
}

/// Server
#[derive(Serialize, Debug, Clone)]
pub struct PostTransitionUpdateField {
    pub add: Option<HashMap<String, Vec<String>>>,
    pub copy: Option<HashMap<String, Vec<String>>>,
    pub edit: Option<HashMap<String, Vec<String>>>,
    pub remove: Option<HashMap<String, Vec<String>>>,
    pub set: Option<HashMap<String, Vec<String>>>,
}

impl Display for Transition {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worklog_tryfrom_all_units_returns_duration_in_seconds() -> Result<(), JiraClientError> {
        let worklogs = vec![
            (60, "1"),
            (60, "1m"),
            (60, "1M"),
            (3600, "1h"),
            (3600, "1H"),
            (3600 * 8, "1d"),
            (3600 * 8, "1D"),
            (3600 * 8 * 5, "1w"),
            (3600 * 8 * 5, "1W"),
        ];

        for (expected_seconds, input) in worklogs {
            let seconds = WorklogDuration::try_from(input.to_string())?.0;
            assert_eq!(expected_seconds.to_string(), seconds);
        }
        Ok(())
    }

    #[test]
    fn worklog_tryfrom_lowercase_unit() -> Result<(), JiraClientError> {
        let wl = WorklogDuration::try_from(String::from("1h"))?;
        assert_eq!(String::from("3600"), wl.to_string());
        Ok(())
    }
    #[test]
    fn worklog_tryfrom_uppercase_unit() -> Result<(), JiraClientError> {
        let wl = WorklogDuration::try_from(String::from("2H"))?;
        assert_eq!(String::from("7200"), wl.to_string());
        Ok(())
    }

    #[test]
    fn worklog_tostring() -> Result<(), JiraClientError> {
        let wl = WorklogDuration::try_from(String::from("1h"))?;
        let expected = String::from("3600");
        assert_eq!(expected, wl.to_string());
        Ok(())
    }

    #[test]
    fn issuekey_tryfrom_uppercase_id() -> Result<(), JiraClientError> {
        let key = String::from("JB-1");
        let issue = IssueKey::try_from(key.clone());
        assert!(issue.is_ok());
        assert_eq!(key, issue?.0);
        Ok(())
    }

    #[test]
    fn issuekey_tryfrom_lowercase_id() {
        let issue = IssueKey::try_from(String::from("jb-1"));
        assert!(issue.is_ok());
    }

    #[test]
    fn issuekey_tostring() {
        let key = String::from("JB-1");
        let issue = IssueKey(key.clone());
        assert_eq!(key, issue.to_string());
    }
}

use crate::JiraClientError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{
    collections::HashMap,
    fmt::{Display, Error, Formatter},
    sync::OnceLock,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
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

        Ok(WorklogDuration(format!("{seconds:.0}")))
    }
}

/// Issue related types
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostIssueQueryBody {
    pub fields: Option<Vec<String>>,
    pub jql: String,
    pub max_results: u32,
    pub start_at: u32,
    /// Expects camelCase and the API is case-sensitive
    pub expand: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostIssueQueryResponseBody {
    /// https://docs.atlassian.com/software/jira/docs/api/REST/7.6.1/#api/2/search
    pub expand: Option<String>,
    pub issues: Option<Vec<Issue>>,
    pub max_results: Option<u32>,
    pub start_at: Option<u32>,
    pub total: Option<u32>,
    /// Some when expanding names on query_issues
    pub names: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub expand: Option<String>,
    pub fields: IssueFields,
    pub id: Option<serde_json::Value>,
    pub key: IssueKey,
    #[serde(alias = "self")]
    pub self_ref: String,
    /// Some when expanding names on query_issue
    pub names: Option<HashMap<String, String>>,

    #[serde(flatten)]
    pub remainder: BTreeMap<String, Value>,
}

/// All fields are optional as it's possible to define what fields you want in the request
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct IssueFields {
    pub assignee: Option<User>,
    pub components: Option<Vec<Component>>,
    pub created: Option<String>,
    pub creator: Option<User>,
    pub description: Option<String>,
    pub duedate: Option<String>,
    pub labels: Option<Vec<String>>,
    pub last_viewed: Option<String>,
    pub reporter: Option<User>,
    pub resolutiondate: Option<String>,
    pub summary: Option<String>,
    pub timeestimate: Option<u32>,
    pub timeoriginalestimate: Option<u32>,
    pub timespent: Option<u32>,
    pub updated: Option<String>,
    pub workratio: Option<i32>,

    // pub project: Project,            //TODO
    // pub issuetype: IssueType,        //TODO
    pub status: Option<Status>,
    // pub comment: CommentContainer,   //TODO
    // pub resolution: Resolution,      //TODO
    // pub priority: Priority,          //TODO
    // pub progress: Progress,          //TODO
    pub subtasks: Option<Vec<SubTask>>,
    // pub issue_links: Vec<Value>,     //TODO
    // pub votes: Votes,                //TODO
    pub worklog: Option<WorkLog>,
    // pub timetracking: TimeTracking,  //TODO
    // pub watches: Watches,            //TODO
    // pub fix_versions: Vec<Version>,  //TODO
    // pub versions: Vec<Version>,      //TODO
    // pub attachment: Vec<Attachment>, //TODO
    #[serde(flatten)]
    pub customfields: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: String,
    pub name: String,
    pub custom: bool,
    pub orderable: bool,
    pub navigable: bool,
    pub searchable: bool,
    pub clause_names: Vec<String>,
    pub schema: Option<FieldSchema>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldSchema {
    pub custom: Option<FieldSchemaType>,
    pub custom_id: Option<u32>,
    pub items: Option<FieldSchemaType>,
    pub system: Option<FieldSchemaType>,
    #[serde(alias = "type")]
    pub field_type: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum FieldSchemaType {
    Any,
    Array,
    Attachment,
    CommentsPage,
    Component,
    Date,
    Datetime,
    Issuelinks,
    Issuetype,
    Number,
    Option,
    Priority,
    Progress,
    Project,
    Resolution,
    Securitylevel,
    Status,
    String,
    Timetracking,
    User,
    Version,
    Votes,
    Watches,
    Worklog,
    #[serde(untagged)]
    Custom(String),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner: User,
    pub jql: String,
    pub view_url: String,
    pub search_url: String,
    pub favourite: bool,
    pub shared_users: FilterSharedUsers,
    // pub subscriptions: FilterSubscriptions
    // pub share_permissions: FilterSharePermissions
}

impl Display for Filter {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}: {}", self.name, self.jql)
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct FilterSharedUsers {
    pub size: u32,
    pub max_results: u32,
    pub start_index: u32,
    pub end_index: u32,
    pub items: Vec<User>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Component {
    pub id: String,
    pub name: String,
    #[serde(alias = "self")]
    pub self_ref: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Status {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub description: String,
    #[serde(alias = "iconUrl")]
    pub icon_url: String,
    pub name: String,
    pub id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct WorkLog {
    #[serde(alias = "startAt")]
    pub start_at: usize,
    #[serde(alias = "maxResults")]
    pub max_results: usize,
    pub total: usize,
    #[serde(alias = "worklogs")]
    pub work_logs: Vec<WorkLogItem>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct WorkLogItem {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub author: Author,
    #[serde(alias = "updateAuthor")]
    pub update_author: Author,
    pub comment: String,
    pub created: String, // TODO: chrono?
    pub updated: String,
    pub started: String,
    #[serde(alias = "timeSpent")]
    pub time_spent: String,
    #[serde(alias = "timeSpentSeconds")]
    pub time_spent_seconds: usize,
    pub id: String,
    #[serde(alias = "issueId")]
    pub issue_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Author {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub name: String,
    pub key: String,
    #[serde(alias = "emailAddress")]
    pub email_address: String,
    #[serde(alias = "avatarUrls")]
    pub avatar_urls: AvatarUrls,
    #[serde(alias = "displayName")]
    pub display_name: String,
    pub active: bool,
    #[serde(alias = "timeZone")]
    pub time_zone: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AvatarUrls {
    #[serde(rename = "48x48")]
    avatar_48x48: String,
    #[serde(rename = "24x24")]
    avatar_24x24: String,
    #[serde(rename = "16x16")]
    avatar_16x16: String,
    #[serde(rename = "32x32")]
    avatar_32x32: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SubTask {
    pub id: String,
    pub key: String,
    #[serde(alias = "self")]
    pub self_ref: String,
    pub fields: SubTaskFields,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SubTaskFields {
    pub summary: String,
    pub status: SubTaskFieldsStatus,
    #[serde(alias = "issuetype")]
    pub issue_type: SubTaskFieldsIssueType,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SubTaskFieldsStatus {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub description: String,
    #[serde(alias = "iconUrl")]
    pub icon_url: String,
    pub name: String,
    pub id: String,
    #[serde(alias = "statusCategory")]
    pub status_category: SubTaskFieldsStatusCategory,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SubTaskFieldsStatusCategory {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub id: u32,
    pub key: String,
    #[serde(alias = "colorName")]
    pub color_name: String,
    pub name: String,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SubTaskFieldsIssueType {
    #[serde(alias = "self")]
    pub self_ref: String,
    pub id: String,
    pub description: String,
    #[serde(alias = "iconUrl")]
    pub icon_url: String,
    pub name: String,
    pub subtask: bool,
    #[serde(alias = "avatarId")]
    pub avatar_id: usize,
}

impl Display for Issue {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "{} {}",
            self.key,
            self.fields
                .summary
                .clone()
                .unwrap_or("summary is None or missing from query response".to_string())
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct IssueKey(String);

impl From<IssueKey> for String {
    fn from(val: IssueKey) -> Self {
        val.0
    }
}

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
        self_ref: String,
        #[serde(alias = "name")]
        value: String,
        id: String,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransitionExpandedFieldsSchema {
    #[serde(alias = "type")]
    pub schema_type: String,
    pub items: Option<String>,
    pub custom: Option<String>,
    pub custom_id: Option<u32>,
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
    pub fields: Option<PostTransitionFieldBody>,
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

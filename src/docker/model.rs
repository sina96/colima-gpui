use serde::Deserialize;

/// Docker's JSON formatter exposes display strings, including Ports and CreatedAt.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
    #[serde(rename = "ID")]
    pub id: String,
    pub names: String,
    pub image: String,
    pub state: String,
    pub status: String,
    #[serde(default)]
    pub ports: String,
    pub created_at: String,
    #[serde(default)]
    pub command: String,
}

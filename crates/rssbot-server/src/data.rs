#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectChatSessionData {
    pub user_id: i64,
    pub target_url: String,
}

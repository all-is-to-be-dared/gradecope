use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqlUser {
    pub id: Uuid,
    pub name: String,
}

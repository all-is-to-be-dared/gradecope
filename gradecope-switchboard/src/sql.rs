use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(sqlx::Type)]
#[sqlx(type_name = "job_state", rename_all = "lowercase")]
pub enum JobState {
    Submitted,
    Started,
    Canceled,
    Finished,
}

#[derive(Debug, Clone)]
pub struct SqlUser {
    pub id: Uuid,
    pub name: String,
}
impl Display for SqlUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{name}#{id}", name = self.name, id = self.id)
    }
}
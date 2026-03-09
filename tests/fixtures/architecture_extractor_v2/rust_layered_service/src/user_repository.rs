pub struct UserRepository;

impl UserRepository {
    pub fn new() -> Self {
        Self
    }

    pub fn find(&self, user_id: &str) -> String {
        format!("user:{user_id}")
    }
}

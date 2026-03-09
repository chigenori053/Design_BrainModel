use crate::user_repository::UserRepository;

pub struct UserService;

impl UserService {
    pub fn new() -> Self {
        Self
    }

    pub fn load_user(&self, user_id: &str) -> String {
        UserRepository::new().find(user_id)
    }
}

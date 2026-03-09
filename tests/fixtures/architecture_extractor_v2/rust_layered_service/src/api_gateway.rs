use crate::user_service::UserService;

pub struct ApiGateway;

impl ApiGateway {
    pub fn new() -> Self {
        Self
    }

    pub fn handle(&self, user_id: &str) -> String {
        UserService::new().load_user(user_id)
    }
}

mod api_gateway;
mod user_repository;
mod user_service;

fn main() {
    let gateway = api_gateway::ApiGateway::new();
    println!("{}", gateway.handle("42"));
}

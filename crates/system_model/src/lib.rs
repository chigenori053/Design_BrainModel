use execution_graph::{ExecutionGraph, ExecutionNode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Database {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageQueue {
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemModel {
    pub services: Vec<Service>,
    pub databases: Vec<Database>,
    pub queues: Vec<MessageQueue>,
}

impl SystemModel {
    pub fn from_execution_graph(graph: &ExecutionGraph) -> Self {
        let mut model = Self::default();
        for node in &graph.nodes {
            match node {
                ExecutionNode::Component(id) => model.services.push(Service { id: *id }),
                ExecutionNode::Database(name) => {
                    model.databases.push(Database { name: name.clone() })
                }
                ExecutionNode::Queue(name) => {
                    model.queues.push(MessageQueue { name: name.clone() })
                }
                ExecutionNode::ExternalService(_) => {}
            }
        }
        model
    }
}
